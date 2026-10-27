//! Dashcam GPS tracks kept beside the videos rather than inside them: the Umino UM-D1M writes
//! `gps.sqlite` at the root of its card (table `gps_track`: Unix seconds, latitude and longitude
//! in millionths of a degree, altitude in decimetres, a point every 3 s; 2026-10-27).

use std::path::Path;

use rusqlite::{Connection, OpenFlags};

/// (Unix seconds, latitude, longitude, altitude in metres).
pub type Point = (i64, f64, f64, f64);

/// Folders above the video looked at for `gps.sqlite` (the card's root is two up from
/// `cam_front/normal/`).
const UP: usize = 4;

/// The points recorded from `start` to `end` (Unix seconds) by the dashcam whose card holds
/// `video`; empty without a track.
pub fn points(video: &Path, start: i64, end: i64) -> Vec<Point> {
    let Some(db) = video.ancestors().skip(1).take(UP).map(|d| d.join("gps.sqlite")).find(|p| p.is_file()) else {
        return Vec::new();
    };
    read(&db, start, end).unwrap_or_default()
}

fn read(db: &Path, start: i64, end: i64) -> rusqlite::Result<Vec<Point>> {
    let conn = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut q = conn.prepare(
        "SELECT timestamp_s, latitude_e6, longitude_e6, altitude_dm FROM gps_track \
         WHERE timestamp_s BETWEEN ?1 AND ?2 ORDER BY timestamp_s",
    )?;
    let rows = q.query_map([start, end], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)? as f64 / 1e6, r.get::<_, i64>(2)? as f64 / 1e6, r.get::<_, i64>(3)? as f64 / 10.0))
    })?;
    rows.collect()
}

/// Average and highest speed in km/h between consecutive points.
// ponytail: no smoothing; a GPS jump between two points shows up as the highest speed
pub fn speeds(points: &[Point]) -> Option<(f64, f64)> {
    let mut distance = 0.0;
    let mut top: f64 = 0.0;
    for w in points.windows(2) {
        let (a, b) = (w[0], w[1]);
        let d = metres(a.1, a.2, b.1, b.2);
        distance += d;
        if b.0 > a.0 {
            top = top.max(d / (b.0 - a.0) as f64 * 3.6);
        }
    }
    let secs = (points.last()?.0 - points.first()?.0) as f64;
    (secs > 0.0).then(|| (distance / secs * 3.6, top))
}

/// Great-circle distance (haversine).
fn metres(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let (dp, dl) = ((lat2 - lat1).to_radians(), (lon2 - lon1).to_radians());
    let h = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * 6_371_000.0 * h.sqrt().asin()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_beside_the_video() {
        let root = std::env::temp_dir().join(format!("eyecansee-gps-{}", std::process::id()));
        let dir = root.join("cam_front/normal");
        std::fs::create_dir_all(&dir).unwrap();
        let conn = Connection::open(root.join("gps.sqlite")).unwrap();
        conn.execute_batch(
            "CREATE TABLE gps_track (timestamp_s INTEGER PRIMARY KEY,latitude_e6 INTEGER NOT NULL,longitude_e6 INTEGER NOT NULL,altitude_dm INTEGER NOT NULL);
             INSERT INTO gps_track VALUES (100, 45464100, 9191900, 1220), (103, 45464280, 9191980, 1222), (106, 45464460, 9192100, 1225), (500, 1, 1, 1);",
        )
        .unwrap();
        drop(conn);
        let p = points(&dir.join("a.mp4"), 100, 110);
        assert_eq!(p.len(), 3);
        assert_eq!(p[0], (100, 45.4641, 9.1919, 122.0));
        let (avg, top) = speeds(&p).unwrap();
        // ~21 m in 3 s each, around the Duomo of Milan (made-up points).
        assert!((20.0..35.0).contains(&avg) && top >= avg, "{avg} {top}");
        assert!(points(&std::env::temp_dir().join("x/y.mp4"), 0, 1).is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }
}
