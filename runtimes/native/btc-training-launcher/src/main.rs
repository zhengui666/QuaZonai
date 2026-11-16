fn configured(
    root: &std::path::Path,
    name: &str,
    config: &serde_json::Value,
) -> anyhow::Result<job::native_configured::ConfiguredNativeRun> {
    let native_config = owner_strategy::training_factory::config(root, name, config)?;
    let name = name.to_owned();
    let config = config.clone();
    Ok(job::native_configured::ConfiguredNativeRun {
        config: native_config,
        install: Box::new(move |engine| {
            owner_strategy::training_factory::install(engine, &name, &config)
        }),
    })
}
fn main() {
    job::native_runner::main_with_hooks(None, Some(configured), None);
}
