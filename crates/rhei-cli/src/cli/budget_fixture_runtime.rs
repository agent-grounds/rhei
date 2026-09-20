// Compile-time-isolated deterministic authority that drives the production
// scheduler, ledger and settlement lifecycle. §AR-neural-admission.8

#[cfg(feature = "budget-fixtures")]
static BUDGET_FIXTURE_ACTIVE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
static BUDGET_LEASES: std::sync::OnceLock<Mutex<HashMap<String, BudgetLease>>> =
    std::sync::OnceLock::new();

#[derive(Clone)]
struct BudgetLease {
    root: PathBuf,
    project_uuid: String,
    reservation_id: String,
    #[cfg_attr(not(feature = "budget-fixtures"), allow(dead_code))]
    qualification: rhei_core::budget::QualifiedLaunch,
    #[cfg(feature = "budget-fixtures")]
    sink: Arc<dyn rhei_tui::EventSink>,
    travel_owner: String,
    transition_receipt_id: String,
}

#[cfg(feature = "budget-fixtures")]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureQualificationDocument {
    schema: String,
    tuple: FixtureTuple,
    grade: String,
    fixture_only: bool,
    obligations: FixtureObligations,
    residual_micro: u64,
    #[serde(default)]
    missing_usage: bool,
}

#[cfg(feature = "budget-fixtures")]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureTuple {
    agent: String,
    provider: String,
    model: String,
    billing: String,
}

#[cfg(feature = "budget-fixtures")]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureObligations {
    capture_barrier: bool,
    committed_in_flight_maximum: bool,
    post_kill_exposure: bool,
    nested_fallback_bound: bool,
}

struct RuntimeBudgetEventSink(Arc<dyn rhei_tui::EventSink>);

impl rhei_core::budget::BudgetEventSink for RuntimeBudgetEventSink {
    fn emit(&self, event: rhei_core::budget::BudgetEvent) {
        self.0.emit(rhei_tui::RunEvent::Budget { event: Box::new(event) });
    }
}

fn budget_fixture_active() -> bool {
    #[cfg(feature = "budget-fixtures")]
    {
        return BUDGET_FIXTURE_ACTIVE.load(std::sync::atomic::Ordering::SeqCst);
    }
    #[cfg(not(feature = "budget-fixtures"))]
    false
}

fn budget_leases() -> &'static Mutex<HashMap<String, BudgetLease>> {
    BUDGET_LEASES.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg(feature = "budget-fixtures")]
fn budget_fixture_document(
    root: &Path,
    resolved: &ResolvedAgent,
) -> MietteResult<FixtureQualificationDocument> {
    let provider = resolved.model_provider.as_deref().unwrap_or("");
    let model = resolved.model_name.as_deref().or(resolved.model.as_deref()).unwrap_or("");
    let path = root.join(".agent-grounds/rhei/qualifications/rhei-test-fixed-2000.json");
    let bytes = fs::read(&path)
        .map_err(|e| file_io_report(&path, "cannot read fixture qualification", e))?;
    let document: FixtureQualificationDocument = serde_json::from_slice(&bytes)
        .map_err(|e| miette!(help = "only the isolated fixture driver supplies this document; regenerate it from the e2e budget support helpers", "invalid fixture qualification {}: {e}", path.display()))?;
    if document.schema != "rhei.test-qualification.v1"
        || document.grade != "contained"
        || !document.fixture_only
        || document.tuple.agent != resolved.agent.id()
        || document.tuple.provider != provider
        || document.tuple.billing != "synthetic-fixed"
        || document.tuple.model != "fixed-2000"
        || !matches!(model, "fixed-2000" | "fixed-2000-a" | "fixed-2000-b")
    {
        return Err(miette!(help = "only the isolated fixture driver supplies this document; regenerate it from the e2e budget support helpers", "fixture qualification does not match the selected synthetic tuple"));
    }
    let o = &document.obligations;
    if !o.capture_barrier {
        return Err(miette!(help = "only the isolated fixture driver supplies this document; regenerate it from the e2e budget support helpers", "qualification missing capture latency or durable capture barrier"));
    }
    if !o.committed_in_flight_maximum {
        return Err(miette!(help = "only the isolated fixture driver supplies this document; regenerate it from the e2e budget support helpers", "qualification missing committed/in-flight request maximum"));
    }
    if !o.post_kill_exposure {
        return Err(miette!(help = "only the isolated fixture driver supplies this document; regenerate it from the e2e budget support helpers", "qualification missing post-kill provider exposure"));
    }
    if !o.nested_fallback_bound {
        return Err(miette!(help = "only the isolated fixture driver supplies this document; regenerate it from the e2e budget support helpers", "qualification missing nested/fallback exposure bound"));
    }
    Ok(document)
}

#[cfg(feature = "budget-fixtures")]
fn budget_fixture_validate_project(root: &Path, dry_run: bool) -> MietteResult<()> {
    let path = root.join(".agent-grounds/rhei/qualifications/rhei-test-fixed-2000.json");
    let bytes = fs::read(&path)
        .map_err(|e| file_io_report(&path, "cannot read fixture qualification", e))?;
    let document: FixtureQualificationDocument = serde_json::from_slice(&bytes)
        .map_err(|e| miette!(help = "only the isolated fixture driver supplies this document; regenerate it from the e2e budget support helpers", "invalid fixture qualification {}: {e}", path.display()))?;
    let o = document.obligations;
    if !o.capture_barrier {
        return Err(miette!(help = "only the isolated fixture driver supplies this document; regenerate it from the e2e budget support helpers", "qualification missing capture latency or durable capture barrier"));
    }
    if !o.committed_in_flight_maximum {
        return Err(miette!(help = "only the isolated fixture driver supplies this document; regenerate it from the e2e budget support helpers", "qualification missing committed/in-flight request maximum"));
    }
    if !o.post_kill_exposure {
        return Err(miette!(help = "only the isolated fixture driver supplies this document; regenerate it from the e2e budget support helpers", "qualification missing post-kill provider exposure"));
    }
    if !o.nested_fallback_bound {
        return Err(miette!(help = "only the isolated fixture driver supplies this document; regenerate it from the e2e budget support helpers", "qualification missing nested/fallback exposure bound"));
    }
    if dry_run {
        println!("would reserve bounded fixture invocation, provider exposure, and ticket travel");
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn budget_admit_agent(
    input: &Path,
    loaded: &LoadedPlan,
    machine: &rhei_validator::StateMachine,
    settings: &RheiSettings,
    opts: &RunOptions,
    task: &rhei_core::ast::Task,
    state_name: &str,
    resolved: &ResolvedAgent,
    plan: &SpawnPlan,
    runtime_dir: &Path,
    task_root: &Path,
    run_id: &str,
    visit_count: u64,
    sink: &Arc<dyn rhei_tui::EventSink>,
) -> MietteResult<BudgetLease> {
    let admitted = (|| {
        let key = plan
            .accounting
            .as_ref()
            .expect("agent plans carry accounting identity")
            .invocation_id
            .clone();
        let cached = { budget_leases().lock().expect("budget lease lock").remove(&key) };
        if let Some(lease) = cached {
            lease.start(sink)?;
            return Ok(lease);
        }

        let project = BudgetProject::resolve(input)?;
        let project_uuid = project.required_identity()?;
        let ticket_identity = budget_ticket_identity(loaded, &task.id, &project_uuid)?;
        let state = machine
            .states
            .get(state_name)
            .ok_or_else(|| miette!(help = "the state machine changed while the run was scheduling; run it again", "state '{state_name}' disappeared before budget admission"))?;
        let threshold = state
            .budget_threshold
            .as_ref()
            .or(settings.defaults.budget_threshold.as_ref())
            .ok_or_else(|| budget_refusal("missing_bound", "missing budget_threshold on the state or in defaults"))?;
        let transition_limit = machine
            .profile_for_node(&task.kind, task.profile_level())
            .and_then(|profile| profile.transition_limit)
            .ok_or_else(|| budget_refusal("missing_bound", "missing finite profile transition_limit"))?;
        let deadline = current_unix_secs().saturating_add(resolved.timeout_secs.unwrap_or(1));
        let invocations = resolve_agent_invocations_for_task(
            machine,
            state_name,
            settings,
            opts,
            Some(task),
        )?;

        let mut attempt_ids = Vec::with_capacity(invocations.len());
        let mut qualifications = Vec::with_capacity(invocations.len());
        for invocation in &invocations {
            let suffix = resolved_agent_log_suffix(invocation, Some(visit_count));
            let arm_plan = plan_agent_spawn_attempt(
                runtime_dir,
                task_root,
                &task.id.to_string(),
                state_name,
                suffix.as_deref(),
                invocation,
                visit_count,
                run_id,
            );
            attempt_ids.push(
                arm_plan
                    .accounting
                    .expect("agent plans carry accounting identity")
                    .invocation_id,
            );
            qualifications.push(budget_qualify_launch(
                &project.root,
                invocation,
                &threshold.currency,
                deadline,
            )?);
        }
        let arms = attempt_ids
            .iter()
            .zip(qualifications.iter())
            .map(|(attempt, qualification)| rhei_core::budget::Arm {
                attempt_identity: attempt,
                threshold,
                // Only a qualification that proved a finite delegation graph
                // lets anything under this invocation reserve.
                // §FS-rhei-budgets.5 §FS-rhei-budgets.6.3
                descendant_envelope_micro: qualification.descendant_envelope_micro(),
                qualification,
            })
            .collect::<Vec<_>>();
        let request = rhei_core::budget::AdmissionRequest {
            ticket_identity: &ticket_identity,
            transition_limit,
            deadline_unix: deadline,
            arms: &arms,
            parent_reservation: None,
        };
        let audit = budget_audit("reserve bounded scheduler work")?;
        let mut journal = rhei_core::budget::Journal::open(&project.root, &project_uuid, true)
            .map_err(budget_error)?;
        journal
            .set_event_sink(Arc::new(RuntimeBudgetEventSink(sink.clone())))
            .map_err(budget_error)?;
        let group = journal.reserve(&request, &audit).map_err(budget_error)?;
        let mut leases = budget_leases().lock().expect("budget lease lock");
        for ((attempt, reservation), qualification) in attempt_ids.into_iter().zip(group.reservation_ids).zip(qualifications) {
            leases.insert(
                attempt,
                BudgetLease {
                    root: project.root.clone(),
                    project_uuid: project_uuid.clone(),
                    reservation_id: reservation,
                    qualification,
                    #[cfg(feature = "budget-fixtures")]
                    sink: sink.clone(),
                    travel_owner: group.travel_reservation_id.clone(),
                    transition_receipt_id: group.transition_receipt_id.clone(),
                },
            );
        }
        let lease = leases.remove(&key).ok_or_else(|| miette!(help = "this is an engine invariant, not an operator error; report it with the run log", "reserved fixture arm identity drift"))?;
        drop(leases);
        // Use the owned transaction; reopening here waits on our own FLOCK.
        // §AR-neural-admission.3
        journal.record_start(&lease.reservation_id, false, &audit).map_err(budget_error)?;
        Ok(lease)
    })();
    if let Err(error) = &admitted {
        budget_emit_admission_halt(input, task, error, sink);
    }
    admitted
}

/// Exact tuple resolution is shared by release and fixture admission. Only
/// the authority source differs. §FS-rhei-budgets.6.1 §AR-neural-admission.1
fn budget_qualify_launch(
    root: &Path,
    invocation: &ResolvedAgent,
    currency: &str,
    deadline: u64,
) -> MietteResult<rhei_core::budget::QualifiedLaunch> {
    #[cfg(not(feature = "budget-fixtures"))]
    let _ = (root, deadline);
    #[cfg(feature = "budget-fixtures")]
    if budget_fixture_active() {
        let document = budget_fixture_document(root, invocation)?;
        return rhei_core::budget::QualifiedLaunch::deterministic_fixture(
            invocation.agent.id(),
            invocation.model_provider.as_deref().unwrap_or(""),
            invocation.model_name.as_deref().or(invocation.model.as_deref()).unwrap_or(""),
            currency,
            document.residual_micro,
            deadline.saturating_add(1),
        )
        .map_err(budget_error);
    }
    let executable = resolve_budget_executable(
        invocation
            .profile
            .command
            .first()
            .ok_or_else(|| miette!(help = "give the agent profile a `command:` whose first element is the client to launch", "qualified agent profile has no executable"))?,
    )?;
    let executable_bytes = fs::read(&executable)
        .map_err(|e| file_io_report(&executable, "cannot read qualified executable", e))?;
    let configuration = serde_json::to_vec(&(
        &invocation.profile.command,
        invocation.agent.id(),
        invocation.mode.as_deref(),
        invocation.model_provider.as_deref(),
        invocation.model_name.as_deref(),
        invocation.model.as_deref(),
        invocation.timeout_secs,
    ))
    .map_err(|error| miette!(help = "a launch that cannot be fingerprinted cannot be matched to a qualification; simplify the profile", "cannot fingerprint launch configuration: {error}"))?;
    let observed = rhei_core::budget::ObservedLaunch {
        executable_sha256: format!("sha256:{:x}", Sha256::digest(executable_bytes)),
        configuration_sha256: format!("sha256:{:x}", Sha256::digest(configuration)),
        rhei_version: env!("CARGO_PKG_VERSION").into(),
        os: std::env::consts::OS.into(),
        architecture: std::env::consts::ARCH.into(),
        agent: invocation.agent.id().into(),
        provider: invocation.model_provider.clone().unwrap_or_default(),
        model: invocation
            .model_name
            .clone()
            .or_else(|| invocation.model.clone())
            .unwrap_or_default(),
    };
    let qualified = rhei_core::budget::Registry::qualify_observed(&observed).map_err(budget_error)?;
    if qualified.currency() != currency {
        return Err(miette!(
            help = "initialize the allowance in the currency the qualification prices, or qualify the transport in the allowance's currency",
            "qualified provider currency {} does not match budget currency {currency}",
            qualified.currency()
        ));
    }
    Ok(qualified)
}

/// Resolve the exact executable bytes before any provider credential is made
/// available. PATH lookup is data discovery, never a qualification claim.
/// §FS-rhei-budgets.6.1 §AR-neural-admission.1
fn resolve_budget_executable(command: &str) -> MietteResult<PathBuf> {
    let requested = PathBuf::from(command);
    if requested.components().count() > 1 || requested.is_absolute() {
        return requested
            .canonicalize()
            .map_err(|e| file_io_report(&requested, "cannot resolve qualified executable", e));
    }
    let path = std::env::var_os("PATH")
        .ok_or_else(|| miette!(help = "set PATH, or name the client by absolute path in the agent profile's `command:`", "PATH is unavailable while resolving qualified executable"))?;
    #[cfg(windows)]
    let extensions = std::env::var_os("PATHEXT")
        .map(|value| {
            value
                .to_string_lossy()
                .split(';')
                .filter(|part| !part.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| vec![".EXE".into(), ".CMD".into(), ".BAT".into()]);
    for directory in std::env::split_paths(&path) {
        let direct = directory.join(command);
        if direct.is_file() {
            return direct
                .canonicalize()
                .map_err(|e| file_io_report(&direct, "cannot resolve qualified executable", e));
        }
        #[cfg(windows)]
        for extension in &extensions {
            let candidate = directory.join(format!("{command}{extension}"));
            if candidate.is_file() {
                return candidate.canonicalize().map_err(|e| {
                    file_io_report(&candidate, "cannot resolve qualified executable", e)
                });
            }
        }
    }
    Err(miette!(help = "install the client, or name it by absolute path in the agent profile's `command:`", "cannot resolve qualified executable '{command}' on PATH"))
}

impl BudgetLease {
    fn journal(&self, sink: &Arc<dyn rhei_tui::EventSink>) -> MietteResult<rhei_core::budget::Journal> {
        let mut journal = rhei_core::budget::Journal::open(&self.root, &self.project_uuid, true)
            .map_err(budget_error)?;
        journal
            .set_event_sink(Arc::new(RuntimeBudgetEventSink(sink.clone())))
            .map_err(budget_error)?;
        Ok(journal)
    }

    fn start(&self, sink: &Arc<dyn rhei_tui::EventSink>) -> MietteResult<()> {
        let audit = budget_audit("transfer bounded process capability")?;
        self.journal(sink)?
            .record_start(&self.reservation_id, false, &audit)
            .map_err(budget_error)
    }

    fn settle_if_captured(
        &self,
        usage_capture: Option<&Path>,
        sink: &Arc<dyn rhei_tui::EventSink>,
    ) -> MietteResult<()> {
        let _ = usage_capture;
        if !budget_fixture_active() {
            self.journal(sink)?.reconcile_inbox(
                &self.reservation_id,
                &budget_audit("collect final provider evidence at invocation completion")?,
            ).map_err(budget_error)?;
            return Ok(());
        }
        #[cfg(not(feature = "budget-fixtures"))]
        let _ = sink;
        #[cfg(feature = "budget-fixtures")]
        {
            let broker = self.fixture_broker()?;
            let proof = broker.fixture_final_bill().map_err(budget_error)?;
            let audit = budget_audit("settle complete synthetic provider final bill")?;
            self.journal(sink)?.settle_captured(&self.reservation_id, proof, &audit).map_err(budget_error)?;
        }
        Ok(())
    }

    fn finish_travel(
        &self,
        task_root: &Path,
        runtime_dir: &Path,
        task_id: &str,
        to_state: &str,
        sink: &Arc<dyn rhei_tui::EventSink>,
    ) -> MietteResult<()> {
        let _ = (runtime_dir, to_state);
        let audit = budget_audit("coordinate bounded outcome and travel")?;
        let mut journal = self.journal(sink)?;
        journal.record_invocation_end(&self.reservation_id, &audit).map_err(budget_error)?;
        // The central entry carries the identity allocated before admission.
        // Never invent a post-transition identity or charge each fanout arm.
        // §AR-neural-admission.6
        if let Some((from, to)) = budget_central_edge(task_root, task_id, &self.transition_receipt_id, &self.travel_owner)? {
            journal.record_transition(&self.travel_owner, &self.transition_receipt_id, &from, &to, &audit).map_err(budget_error)
        } else {
            journal.release_completed_group_travel(&self.travel_owner, &audit).map_err(budget_error)
        }
    }
}
