use bevy::prelude::*;
use bv_bevy::camera::{CameraFrameServerPlugin, DebugCameraPlugin, DroneCameraPlugin};
use bv_bevy::capture::CapturePlugin;
use bv_bevy::cli::Cli;
use bv_bevy::sim::SimReceiverPlugin;
use bv_bevy::vehicle::VehiclePlugin;
use bv_bevy::worlds::WorldPlugin;

fn main() -> AppExit {
    let cli = Cli::parse_or_exit();
    let home = cli.world.px4_home();
    if cli.px4_home {
        println!("PX4_HOME_LAT={}", home.latitude);
        println!("PX4_HOME_LON={}", home.longitude);
        println!("PX4_HOME_ALT=0");
        return AppExit::Success;
    }

    let camera = DroneCameraPlugin::from_env();
    let frames = CameraFrameServerPlugin::from_env();
    let sim = SimReceiverPlugin::from_env();
    eprintln!("world        {}", cli.world.name());
    eprintln!("camera       {}", camera.describe());
    eprintln!("frame server {}", frames.describe());
    eprintln!("sim state    {}", sim.describe());
    eprintln!("PX4 home     {:.6}, {:.6} (./px4.sh {})", home.latitude, home.longitude, cli.world.name());

    let mut app = App::new();
    app.add_plugins(DefaultPlugins)
        .add_plugins((camera, frames, sim))
        .add_plugins((WorldPlugin(cli.world), DebugCameraPlugin, VehiclePlugin));
    if let Some(path) = cli.screenshot {
        app.add_plugins(CapturePlugin { path });
    }
    app.run()
}
