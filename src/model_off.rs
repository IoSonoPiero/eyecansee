//! OFF (Geomview, shape collections such as Princeton's): a text list of vertices and of
//! polygons, with colours per face or, in COFF, per vertex.

use crate::model::{Builder, Mesh};

pub fn read(bytes: &[u8]) -> Result<Mesh, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "not a text OFF file")?;
    let mut lines = text.lines().map(|l| l.split('#').next().unwrap_or("").split_whitespace().collect::<Vec<_>>()).filter(|l| !l.is_empty());
    let mut head = lines.next().ok_or("empty file")?;
    let first = head.remove(0);
    // "OFF8 6 12": the first count stuck to the keyword.
    let (kind, stuck) = first.find("OFF").map(|i| first.split_at(i + 3)).ok_or("not an OFF file")?;
    if !stuck.is_empty() {
        head.insert(0, stuck);
    }
    let Some(prefix) = kind.strip_suffix("OFF") else { return Err("not an OFF file".into()) };
    if prefix.contains('4') || prefix.contains('n') || head.first() == Some(&"BINARY") {
        return Err("4D and binary OFF files are not supported".into());
    }
    let counts = if head.is_empty() { lines.next().ok_or("no counts")? } else { head };
    let num = |s: Option<&&str>| s.and_then(|s| s.parse::<usize>().ok()).ok_or("bad counts");
    let (nv, nf) = (num(counts.first())?, num(counts.get(1))?);
    // After x y z: a normal (N), then a colour (C), then texture coordinates (ST).
    let color_at = 3 + if prefix.contains('N') { 3 } else { 0 };
    let mut pos = Vec::with_capacity(nv.min(1 << 24));
    let mut vcolor = Vec::new();
    for _ in 0..nv {
        let l = lines.next().ok_or("fewer vertices than announced")?;
        let f: Vec<f32> = l.iter().map(|s| s.parse().unwrap_or(f32::NAN)).collect();
        pos.push([f[0], *f.get(1).unwrap_or(&f32::NAN), *f.get(2).unwrap_or(&f32::NAN)]);
        if prefix.contains('C') {
            vcolor.push(color(f.get(color_at..color_at + 3).unwrap_or(&[])));
        }
    }
    let (mut tris, mut colors) = (Vec::new(), Vec::new());
    for l in lines.take(nf) {
        let Some(n) = l.first().and_then(|s| s.parse::<usize>().ok()) else { continue };
        let idx: Vec<u32> = l.iter().skip(1).take(n).filter_map(|s| s.parse().ok()).collect();
        let rest: Vec<f32> = l.iter().skip(1 + n).filter_map(|s| s.parse().ok()).collect();
        let face = color(&rest);
        for k in 1..idx.len().saturating_sub(1) {
            let t = [idx[0], idx[k], idx[k + 1]];
            tris.push(t);
            colors.push(face.or_else(|| vcolor.get(t[0] as usize).copied().flatten()));
        }
    }
    let mut b = Builder::default();
    b.add(pos, tris, |i| colors[i])?;
    b.finish(false)
}

/// Three numbers from 0 to 1, or from 0 to 255 when any is above 1.
fn color(c: &[f32]) -> Option<[u8; 3]> {
    let c: [f32; 3] = c.get(..3)?.try_into().ok()?;
    let scale = if c.iter().any(|&v| v > 1.0) { 1.0 } else { 255.0 };
    Some(c.map(|v| (v * scale).clamp(0.0, 255.0) as u8))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn faces_with_colours_and_a_quad() {
        let m = read(b"OFF\n# a square and a triangle\n5 2 0\n0 0 0\n1 0 0\n1 1 0\n0 1 0\n2 2 2\n4 0 1 2 3 255 0 0\n3 1 2 4 0.0 1.0 0.0 1.0\n").unwrap();
        assert_eq!(m.tris.len(), 3);
        assert_eq!(m.colors, vec![[255, 0, 0], [255, 0, 0], [0, 255, 0]]);
        assert!(read(b"COFF 3 1 0\n0 0 0 1 1 1 1\n1 0 0 1 1 1 1\n0 1 0 1 1 1 1\n3 0 1 2\n").unwrap().colors == vec![[255, 255, 255]]);
        assert!(read(b"ply\n").is_err());
        assert_eq!(read(b"OFF3 1 0\n0 0 0\n1 0 0\n0 1 0\n3 0 1 2\n").unwrap().tris.len(), 1);
    }
}
