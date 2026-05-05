use ext_php_rs::prelude::*;
use mimalloc::MiMalloc;

mod data;
mod json;
mod knn;
mod vector;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

/// Computes the fraud count (0..=5) for a given JSON payload body.
/// Returns a count of fraud labels among the 5 nearest neighbors.
/// Returns 0 on parse failure.
#[php_function]
pub fn rinha_fraud_score(body: String) -> u32 {
    let bytes = body.as_bytes();
    let payload = match json::parse(bytes) {
        Some(p) => p,
        None => return 0,
    };
    let v = vector::vectorize(&payload);
    let count = knn::knn5_fraud_count(&v, data::dataset());
    count as u32
}

/// Module startup function — runs once per PHP process at load time.
extern "C" fn module_startup(_ty: i32, _mod_num: i32) -> i32 {
    let t0 = std::time::Instant::now();
    if let Err(e) = data::init() {
        eprintln!("rinha_rs: data::init failed: {}", e);
        return 1; // FAILURE
    }
    let t_mmap = t0.elapsed();
    let t1 = std::time::Instant::now();
    knn::warmup();
    eprintln!(
        "rinha_rs: mmap+init {:?}, warmup {:?}",
        t_mmap,
        t1.elapsed()
    );
    0 // SUCCESS
}

#[php_module]
pub fn get_module(module: ModuleBuilder) -> ModuleBuilder {
    module.startup_function(module_startup)
}
