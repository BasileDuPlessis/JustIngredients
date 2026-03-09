fn main() {
    let text = "- 2 cups flour\n* 1 tbsp sugar\n• 500g butter\n  3 eggs";
    println!("Full text: {:?}", text);
    println!("Lines:");
    for (i, line) in text.lines().enumerate() {
        println!("  {}: {:?}", i, line);
    }
}
