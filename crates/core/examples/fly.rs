use rocket_core::{guide_to_pad, Cfg, Status, World};

fn main() {
    let mut cfg = Cfg::default();
    cfg.infinite_fuel = true;
    let mut w = World::single(cfg);
    let pad = w.pads[0];
    for step in 0..6000 {
        let a = guide_to_pad(&cfg, &w.rockets[0], &pad);
        w.step(&[a]);
        let r = &w.rockets[0];
        if step % 60 == 0 || r.status != Status::Flying {
            println!(
                "t={:5.1} x={:6.2} y={:6.2} vx={:6.2} vy={:6.2} th={:6.2} om={:6.2} legs={} st={:?} dist={:.2}",
                w.time, r.x, r.y, r.vx, r.vy, r.th, r.om, r.legs_out, r.status,
                (r.x - pad.cx).hypot(r.y - pad.y)
            );
        }
        if r.status != Status::Flying {
            break;
        }
    }
}
