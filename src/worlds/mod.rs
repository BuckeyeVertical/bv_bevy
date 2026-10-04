//! The worlds Bevy can draw. Pick one with `cargo run -- <world>`.
//!
//! To add a world:
//! 1. Write `worlds/<name>.rs` with a plugin that inserts [`DebugCameraStart`],
//!    adds a sky (usually [`shared::DaylightPlugin`]), spawns the world and sets
//!    [`WorldLoad::Ready`] once it is loaded.
//! 2. Add a variant to [`WorldChoice`] (its doc comment is the `--help` text)
//!    and follow the compiler to the `match` arms below.
//! 3. Add it to `docs/Run.md`.

mod forest;
mod gazebo_boxes;
mod grass_targets;
mod minimal;
mod satellite_map;
pub mod shared;
mod suas_2026;

use std::path::PathBuf;

use bevy::prelude::*;

use crate::geo::{self, GeoPoint};

pub use forest::Quality;

#[derive(clap::Subcommand, Clone, Debug)]
pub enum WorldChoice {
    /// Flat grass with a grid, a landing pad and three obstacles.
    #[command(name = "minimal")]
    Minimal,
    /// Coloured boxes where the objects of the PX4 Gazebo world (bv_mission.sdf) stand.
    #[command(name = "gazebo_boxes")]
    GazeboBoxes,
    /// Grass with a mannequin and a tent along a scan line: a quick vision test.
    #[command(name = "grass_targets")]
    GrassTargets,
    /// The SUAS 2026 competition field: boundaries, lap route, targets and trees.
    #[command(name = "suas_2026")]
    Suas2026,
    /// A forest clearing with a launch pad, road, shed and power line.
    #[command(name = "forest")]
    Forest(ForestOptions),
    /// Real satellite imagery and terrain around a latitude/longitude.
    #[command(name = "satellite_map")]
    SatelliteMap(SatelliteMapOptions),
}

#[derive(clap::Args, Clone, Debug)]
pub struct ForestOptions {
    /// Trade detail for frame rate.
    #[arg(long, value_enum, default_value_t)]
    pub quality: Quality,
    /// Turn sun shadows off (for weak GPUs).
    #[arg(long)]
    pub no_shadows: bool,
    /// Screenshot fixed viewpoints into DIR with their frame times, then exit.
    #[arg(long, value_name = "DIR")]
    pub tour: Option<PathBuf>,
}

#[derive(clap::Args, Clone, Debug)]
pub struct SatelliteMapOptions {
    /// Latitude of the map centre (default: Tuttle Park, Columbus).
    #[arg(long, allow_hyphen_values = true, default_value_t = geo::TUTTLE_PARK.latitude)]
    pub lat: f64,
    /// Longitude of the map centre.
    #[arg(long, allow_hyphen_values = true, default_value_t = geo::TUTTLE_PARK.longitude)]
    pub lon: f64,
}

impl WorldChoice {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::GazeboBoxes => "gazebo_boxes",
            Self::GrassTargets => "grass_targets",
            Self::Suas2026 => "suas_2026",
            Self::Forest(_) => "forest",
            Self::SatelliteMap(_) => "satellite_map",
        }
    }

    /// Where PX4 believes the world origin is. The bv_ws mission configs for
    /// the flat worlds and the forest fly around the SUAS field home.
    pub fn px4_home(&self) -> GeoPoint {
        match self {
            Self::SatelliteMap(options) => GeoPoint::new(options.lat, options.lon),
            Self::Minimal
            | Self::GazeboBoxes
            | Self::GrassTargets
            | Self::Suas2026
            | Self::Forest(_) => geo::SUAS_2026_FIELD,
        }
    }
}

/// Whether the chosen world has finished loading.
#[derive(States, Default, Debug, Clone, PartialEq, Eq, Hash)]
pub enum WorldLoad {
    #[default]
    Loading,
    Ready,
}

/// Where the debug camera starts.
#[derive(Resource, Clone, Copy)]
pub struct DebugCameraStart(pub Transform);

pub struct WorldPlugin(pub WorldChoice);

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<WorldLoad>();
        match &self.0 {
            WorldChoice::Minimal => app.add_plugins(minimal::MinimalPlugin),
            WorldChoice::GazeboBoxes => app.add_plugins(gazebo_boxes::GazeboBoxesPlugin),
            WorldChoice::GrassTargets => app.add_plugins(grass_targets::GrassTargetsPlugin),
            WorldChoice::Suas2026 => app.add_plugins(suas_2026::Suas2026Plugin),
            WorldChoice::Forest(options) => app.add_plugins(forest::ForestPlugin {
                quality: options.quality,
                shadows: !options.no_shadows,
                tour: options.tour.clone(),
            }),
            WorldChoice::SatelliteMap(options) => app.add_plugins(satellite_map::SatelliteMapPlugin {
                anchor: GeoPoint::new(options.lat, options.lon),
            }),
        };
    }
}

/// For worlds that are complete as soon as they spawn.
fn ready(mut next: ResMut<NextState<WorldLoad>>) {
    next.set(WorldLoad::Ready);
}
