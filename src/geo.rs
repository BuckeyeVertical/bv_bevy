//! Places on Earth the worlds are built around.

/// A WGS84 latitude/longitude in degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeoPoint {
    pub latitude: f64,
    pub longitude: f64,
}

impl GeoPoint {
    pub const fn new(latitude: f64, longitude: f64) -> Self {
        Self {
            latitude,
            longitude,
        }
    }
}

/// Home of the SUAS 2026 competition field.
pub const SUAS_2026_FIELD: GeoPoint = GeoPoint::new(36.214_675, -96.006_555_56);

/// Tuttle Park, Columbus, Ohio.
pub const TUTTLE_PARK: GeoPoint = GeoPoint::new(40.011_977, -83.015_751);
