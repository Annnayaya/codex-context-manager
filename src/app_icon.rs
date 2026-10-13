use std::f32::consts::TAU;

/// Render the application icon without a runtime image decoder or embedded asset.
pub fn rgba(size: u32) -> Vec<u8> {
    let mut pixels = vec![0; size as usize * size as usize * 4];
    let gear = gear_outline();
    const SAMPLES: u32 = 4;

    for y in 0..size {
        for x in 0..size {
            let mut opaque = 0_u32;
            let mut white = 0_u32;
            for sample_y in 0..SAMPLES {
                for sample_x in 0..SAMPLES {
                    let px =
                        (x as f32 + (sample_x as f32 + 0.5) / SAMPLES as f32) / size as f32 - 0.5;
                    let py =
                        (y as f32 + (sample_y as f32 + 0.5) / SAMPLES as f32) / size as f32 - 0.5;
                    if !inside_tile(px, py, 0.46, 0.13) {
                        continue;
                    }
                    opaque += 1;
                    let radius_squared = px * px + py * py;
                    let on_gear = (0.112_f32.powi(2)..=0.34_f32.powi(2)).contains(&radius_squared)
                        && inside_polygon(px, py, &gear);
                    let on_border = !inside_tile(px, py, 0.445, 0.115);
                    if !on_gear && !on_border {
                        white += 1;
                    }
                }
            }
            let offset = ((y * size + x) * 4) as usize;
            if let Some(shade) = (255 * white).checked_div(opaque) {
                pixels[offset..offset + 3].fill(shade as u8);
                pixels[offset + 3] = (255 * opaque / (SAMPLES * SAMPLES)) as u8;
            }
        }
    }
    pixels
}

fn inside_tile(x: f32, y: f32, half_width: f32, corner_radius: f32) -> bool {
    let dx = (x.abs() - (half_width - corner_radius)).max(0.0);
    let dy = (y.abs() - (half_width - corner_radius)).max(0.0);
    dx * dx + dy * dy <= corner_radius * corner_radius
}

fn gear_outline() -> Vec<(f32, f32)> {
    let mut points = Vec::with_capacity(48);
    for tooth in 0..8 {
        for (angle, radius) in [
            (-22.5_f32, 0.245),
            (-14.0, 0.245),
            (-10.0, 0.34),
            (10.0, 0.34),
            (14.0, 0.245),
            (22.5, 0.245),
        ] {
            let direction = tooth as f32 * TAU / 8.0 + angle.to_radians();
            points.push((direction.cos() * radius, direction.sin() * radius));
        }
    }
    points
}

fn inside_polygon(x: f32, y: f32, polygon: &[(f32, f32)]) -> bool {
    let mut inside = false;
    let mut previous = polygon[polygon.len() - 1];
    for &(px, py) in polygon {
        if (py > y) != (previous.1 > y) && x < (previous.0 - px) * (y - py) / (previous.1 - py) + px
        {
            inside = !inside;
        }
        previous = (px, py);
    }
    inside
}
