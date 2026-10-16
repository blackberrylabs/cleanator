//! Desktop application entry point. The iced interface is built in PLAN step 3, after a
//! one-day spike validated by the author. Until then this binary only reports the core
//! version so the workspace builds as a whole.

fn main() {
    println!(
        "cleanator {} — desktop UI not built yet (see PLAN.md, step 3)",
        cleanator_core::VERSION
    );
}
