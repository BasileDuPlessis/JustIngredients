use just_ingredients::text_processing::MeasurementDetector;

fn main() {
    let detector = MeasurementDetector::new().unwrap();
    let text = "temp: -2 cups flour";
    let matches = detector.extract_ingredient_measurements(text);

    println!("Testing measurement detection on: '{}'", text);
    for (i, m) in matches.iter().enumerate() {
        println!("Match {}: quantity='{}', measurement='{:?}', ingredient_name='{}', start_pos={}, end_pos={}",
                 i, m.quantity, m.measurement, m.ingredient_name, m.start_pos, m.end_pos);
    }

    // Test the parse function
    let result = just_ingredients::validation::parse_ingredient_from_text("-2 cups flour");
    match result {
        Ok(m) => println!("Parsed result: quantity='{}', measurement='{:?}', ingredient_name='{}', start_pos={}, end_pos={}",
                         m.quantity, m.measurement, m.ingredient_name, m.start_pos, m.end_pos),
        Err(e) => println!("Parse error: {}", e),
    }
}
