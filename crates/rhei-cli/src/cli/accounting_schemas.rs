// Published accounting contracts embedded into every installed binary.

// §FS-rhei-cost-accounting.8.1

const PUBLISHED_ACCOUNTING_SCHEMAS: &[(&str, &str)] = &[
    (
        "rhei.accounting.cost.v1",
        include_str!("../../schemas/rhei.accounting.cost.v1.schema.json"),
    ),
    (
        "rhei.accounting.invocation.v1",
        include_str!("../../schemas/rhei.accounting.invocation.v1.schema.json"),
    ),
    (
        "rhei.accounting.prices.v1",
        include_str!("../../schemas/rhei.accounting.prices.v1.schema.json"),
    ),
    (
        "rhei.accounting.summary.v1",
        include_str!("../../schemas/rhei.accounting.summary.v1.schema.json"),
    ),
    (
        "rhei.accounting.task.v1",
        include_str!("../../schemas/rhei.accounting.task.v1.schema.json"),
    ),
    (
        "rhei.accounting.usage.v1",
        include_str!("../../schemas/rhei.accounting.usage.v1.schema.json"),
    ),
    // The evidence documents and the summary checkpoint, beside the accounting
    // ones. §FS-rhei-evidence §FS-rhei-summary.6
    (
        "rhei.evidence.event.v1",
        include_str!("../../schemas/rhei.evidence.event.v1.schema.json"),
    ),
    (
        "rhei.evidence.invocation.v1",
        include_str!("../../schemas/rhei.evidence.invocation.v1.schema.json"),
    ),
    (
        "rhei.evidence.manifest.v1",
        include_str!("../../schemas/rhei.evidence.manifest.v1.schema.json"),
    ),
    // A seal generation, which an evidence start record names. §FS-rhei-seal.3
    (
        "rhei.seal.v1",
        include_str!("../../schemas/rhei.seal.v1.schema.json"),
    ),
    (
        "rhei.summary.checkpoint.v1",
        include_str!("../../schemas/rhei.summary.checkpoint.v1.schema.json"),
    ),
];

fn accounting_schema_command(name: Option<&str>) -> MietteResult<()> {
    let Some(name) = name else {
        for (schema_id, _) in PUBLISHED_ACCOUNTING_SCHEMAS {
            println!("{schema_id}");
        }
        return Ok(());
    };

    let Some((_, schema)) =
        PUBLISHED_ACCOUNTING_SCHEMAS.iter().find(|(schema_id, _)| *schema_id == name)
    else {
        return Err(miette!(
            help = "run `rhei schema --list` to list published accounting schema ids",
            "unknown accounting schema id '{name}'"
        ));
    };
    std::io::stdout()
        .write_all(schema.as_bytes())
        .map_err(|err| {
            miette!(
                help = "check that stdout is writable and retry the schema command",
                "failed to write accounting schema '{name}': {err}"
            )
        })
}
