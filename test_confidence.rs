use leptess::LepTess;

fn main() {
    let mut tess = LepTess::new(None, "eng").expect("LepTess should initialize successfully");
    // Try to see what methods are available
    println!("Methods available on LepTess:");
    // This won't compile but will show available methods
    tess.mean_confidence();
}
