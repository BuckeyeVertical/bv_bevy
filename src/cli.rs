//! Command line: `cargo run -- <world> [options]`.

use std::path::PathBuf;

use clap::Parser;

use crate::worlds::WorldChoice;

#[derive(Parser, Debug)]
#[command(
    name = "bv_bevy",
    about = "Draws a world and the drone camera for the PX4/Gazebo simulator.",
    arg_required_else_help = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub world: WorldChoice,

    /// Save a screenshot once the world has loaded, then exit.
    #[arg(long, global = true, value_name = "PNG")]
    pub screenshot: Option<PathBuf>,

    /// Print this world's PX4 home as shell variables and exit (used by ./px4.sh).
    #[arg(long, global = true, hide = true)]
    pub px4_home: bool,
}

/// Old world names, so stale scripts and habits fail with a pointer instead
/// of "unrecognized subcommand".
const RENAMED_WORLDS: &[(&str, &str)] = &[
    ("bv_mission", "gazebo_boxes"),
    ("missionTest", "grass_targets"),
    ("SUAS", "suas_2026"),
    ("suas", "suas_2026"),
    ("provingGround", "forest"),
    ("proving_ground", "forest"),
    ("geoTiles", "satellite_map"),
    ("geo_tiles", "satellite_map"),
];

/// Environment variables that used to configure the worlds. They are now
/// options, and a leftover export must not be silently ignored.
const RETIRED_VARIABLES: &[(&str, &str)] = &[
    ("BV_WORLD_PROFILE", "cargo run -- <world>"),
    ("BV_ENV_QUALITY", "cargo run -- forest --quality <low|medium|high>"),
    ("BV_ENV_SHADOWS", "cargo run -- forest --no-shadows"),
    ("BV_ENV_PREVIEW", "cargo run -- forest --tour <dir>"),
    ("BV_ENV_PREVIEW_SIZE", "cargo run -- forest --tour <dir>"),
    ("BV_GEO_LAT", "cargo run -- satellite_map --lat <deg> --lon <deg>"),
    ("BV_GEO_LON", "cargo run -- satellite_map --lat <deg> --lon <deg>"),
    ("BV_GEO_SCREENSHOT", "cargo run -- <world> --screenshot <png>"),
];

impl Cli {
    /// Parse the command line, exiting with a message on anything outdated.
    pub fn parse_or_exit() -> Self {
        let retired: Vec<String> = RETIRED_VARIABLES
            .iter()
            .filter(|(name, _)| std::env::var_os(name).is_some())
            .map(|(name, instead)| format!("  {name} is no longer read; use `{instead}`"))
            .collect();
        if !retired.is_empty() {
            eprintln!("error: unset these environment variables:\n{}", retired.join("\n"));
            std::process::exit(2);
        }
        if let Some(message) = std::env::args().nth(1).as_deref().and_then(renamed_world) {
            eprintln!("error: {message}");
            std::process::exit(2);
        }
        Self::parse()
    }
}

fn renamed_world(arg: &str) -> Option<String> {
    RENAMED_WORLDS
        .iter()
        .find(|(old, _)| *old == arg)
        .map(|(old, new)| format!("the `{old}` world is now `{new}`: cargo run -- {new}"))
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn command_line_is_well_formed() {
        Cli::command().debug_assert();
    }

    #[test]
    fn every_world_parses_by_its_name() {
        for name in ["minimal", "gazebo_boxes", "grass_targets", "suas_2026", "forest", "satellite_map"] {
            let cli = Cli::try_parse_from(["bv_bevy", name]).unwrap();
            assert_eq!(cli.world.name(), name);
        }
    }

    #[test]
    fn world_options_parse() {
        let cli = Cli::try_parse_from(["bv_bevy", "forest", "--quality", "low", "--no-shadows"]).unwrap();
        let WorldChoice::Forest(forest) = cli.world else { panic!() };
        assert_eq!(forest.quality, crate::worlds::Quality::Low);
        assert!(forest.no_shadows);

        let cli = Cli::try_parse_from(["bv_bevy", "satellite_map", "--lat", "36.1", "--lon", "-96.2"]).unwrap();
        let home = cli.world.px4_home();
        assert_eq!((home.latitude, home.longitude), (36.1, -96.2));
    }

    #[test]
    fn old_world_names_point_to_new_ones() {
        assert!(renamed_world("provingGround").unwrap().contains("forest"));
        assert!(renamed_world("forest").is_none());
        for (_, new) in RENAMED_WORLDS {
            assert!(Cli::try_parse_from(["bv_bevy", new]).is_ok(), "{new}");
        }
    }
}
