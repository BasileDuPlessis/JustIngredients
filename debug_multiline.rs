use just_ingredients::text_processing::MeasurementDetector;

fn main() {
    let detector = MeasurementDetector::new().expect("MeasurementDetector should initialize successfully");
    let text = "8 tablespoons unsalted butter, cold and\ncubed (See note.)";
    
    println!("Input text:");
    for (i, line) in text.lines().enumerate() {
        println!("  {}: '{}'", i, line);
    }
    
    let matches = detector.extract_ingredient_measurements(text);
    
    println!("\nFound {} matches:", matches.len());
    for (i, m) in matches.iter().enumerate() {
        println!("  {}: '{}' '{}' '{}' (line {})", 
                i+1, m.quantity, m.measurement.as_deref().unwrap_or(""), m.ingredient_name, m.line_number);
    }
}
