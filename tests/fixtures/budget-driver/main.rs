//! Separately compiled provider-free driver for bounded-runtime scenarios.
//! It links the ordinary CLI scheduler with deterministic authority that does
//! not exist in either installed binary. §FS-rhei-budgets.12 §AR-neural-admission.8

fn main() {
    rhei_cli::run_budget_fixture();
}
