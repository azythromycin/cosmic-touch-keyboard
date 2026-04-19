mod layout;
mod shell_state;
mod ui_layer_shell;
mod vk;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tracing::info!("cosmic-touch-keyboard starting");

    let wl = vk::spawn_wayland_thread();
    ui_layer_shell::run(wl.vk_tx, true)
}
