//! `cargo run -p orion-app --features snap --bin orion-snap -- <pasta> [filtro]`

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = std::path::PathBuf::from(args.next().unwrap_or_else(|| "snaps".into()));
    let filter = args.next();
    std::fs::create_dir_all(&dir).ok();
    for (name, size, app) in orion_app::snap::scenarios() {
        if filter.as_deref().map(|f| name.contains(f)).unwrap_or(true) {
            let out = dir.join(format!("{name}.png"));
            orion_app::snap::render(&app, size, &out);
            println!("{}", out.display());
            if name == "00-onboarding" {
                for (section, offset) in [("apis", 1000.0), ("backup", 1900.0), ("importacoes", 3000.0)] {
                    let out = dir.join(format!("{name}-{section}.png"));
                    orion_app::snap::render_scrolled(&app, size, &out, offset);
                    println!("{}", out.display());
                }
            }
        }
    }
}
