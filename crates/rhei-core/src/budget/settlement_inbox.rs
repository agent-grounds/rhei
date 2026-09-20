//! Automatic collection uses precisely the operator reconciliation verifier;
//! client output and unsigned usage cannot release exposure. §FS-rhei-budgets.7

use super::{Audit, BudgetError, Journal, Registry, Result};
use std::io::Read;

impl Journal {
    pub fn reconcile_inbox(&mut self, reservation: &str, audit: &Audit) -> Result<bool> {
        let id = reservation
            .strip_prefix("reservation:")
            .ok_or_else(|| BudgetError::corrupt("invalid settlement reservation identity"))?;
        super::types::uuid(id)?;
        let r = self
            .state
            .reservations
            .get(reservation)
            .ok_or_else(|| BudgetError::corrupt("settlement reservation is absent"))?;
        let path = self
            .path
            .parent()
            .expect("journal parent")
            .join("provider-evidence")
            .join(format!("{id}.json"));
        let file = match std::fs::File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
        };
        let mut bytes = Vec::new();
        file.take(1_048_577).read_to_end(&mut bytes)?;
        if bytes.len() > 1_048_576 {
            return Err(BudgetError::new("evidence_refused", "provider export exceeds size limit"));
        }
        let mut signature = String::new();
        std::fs::File::open(path.with_extension("json.sig"))?
            .take(130)
            .read_to_string(&mut signature)?;
        let signature = signature.trim();
        if signature.len() != 128
            || !signature.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return Err(BudgetError::new(
                "evidence_refused",
                "invalid provider signature encoding",
            ));
        }
        let mut detached = [0u8; 64];
        for (index, pair) in signature.as_bytes().chunks_exact(2).enumerate() {
            detached[index] = u8::from_str_radix(std::str::from_utf8(pair).expect("ASCII"), 16)
                .map_err(BudgetError::corrupt)?;
        }
        let tuple = serde_json::from_value(r.payload["qualification"].clone())?;
        let authority = Registry::evidence_authority(
            &tuple,
            &self.project_id,
            &self.snapshot()?.allowance.spend.currency,
        )?;
        let proof = authority.verify_reconciliation(
            &bytes,
            &detached,
            reservation,
            super::replay::string(&r.payload, "attempt_identity")?,
        )?;
        self.reconcile_verified(proof, audit)?;
        Ok(true)
    }
}
