use font8x8::UnicodeFonts;
fn main() {
    let ch = '┌'; 
    let g = match font8x8::BASIC_FONTS.get(ch) {
        Some(font) => Some(font),
        None => font8x8::BOX_FONTS.get(ch),
    };
    println!("{:?}", g.is_some());
}
