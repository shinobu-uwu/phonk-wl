use std::time::Duration;

use anyhow::Result;
use smithay_client_toolkit::reexports::{
    calloop::timer::{TimeoutAction, Timer},
    calloop_wayland_source::WaylandSource,
};
use smithay_client_toolkit::{
    compositor::CompositorState, output::OutputState, reexports::calloop::EventLoop,
    registry::RegistryState, shell::wlr_layer::LayerShell, shm::Shm,
};
use wayland_client::{Connection, QueueHandle, globals::registry_queue_init};

use crate::app::App;

mod app;

fn main() -> Result<()> {
    let conn = Connection::connect_to_env()?;
    let (globals, event_queue) = registry_queue_init(&conn)?;
    let qh: QueueHandle<App> = event_queue.handle();

    let compositor_state = CompositorState::bind(&globals, &qh)?;
    let layer_shell = LayerShell::bind(&globals, &qh)?;
    let output_state = OutputState::new(&globals, &qh);
    let registry_state = RegistryState::new(&globals);
    let shm = Shm::bind(&globals, &qh)?;

    let mut app = App::new(
        output_state,
        layer_shell,
        shm,
        compositor_state,
        registry_state,
    )?;

    let mut event_loop: EventLoop<App> = EventLoop::try_new()?;
    let loop_handle = event_loop.handle();

    let wayland_source = WaylandSource::new(conn, event_queue);
    loop_handle.insert_source(wayland_source, |_, queue, app| queue.dispatch_pending(app))?;
    let timer = Timer::from_duration(Duration::from_secs(2));
    loop_handle
        .insert_source(timer, |_deadline, _metadata, app| {
            app.toggle_overlay();
            TimeoutAction::ToDuration(Duration::from_secs(5))
        })
        .unwrap();

    event_loop.run(None, &mut app, |_| {})?;

    Ok(())
}
