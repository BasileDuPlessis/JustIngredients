use just_ingredients::text_processing::MeasurementDetector;

fn main() {
    let detector = MeasurementDetector::new().unwrap();
    let text = "2 oeufs";
    let matches = detector.extract_ingredient_measurements(text);
    
    println!("Testing measurement detection on: '{}'", text);
    for (i, m) in matches.iter().enumerate() {
        println!("Match {}: quantity='{}', measurement='{:?}', ingredient_name='{}'", 
                 i, m.quantity, m.measurement, m.ingredient_name);
    }
    
    // Test the parse function
    let result = just_ingredients::bot::dialogue_manager::parse_ingredient_from_text("2 oeufs");
    match result {
        Ok(m) => println!("Parsed result: quantity='{}', measurement='{:?}', ingredient_name='{}'", 
                         m.quantity, m.measurement, m.ingredient_name),
        Err(e) => println!("Parse error: {}", e),
    }
}
