use font8x8::UnicodeFonts;
fn main() {
    let ch = '┌'; 
    let g = font8x8::BASIC_FONTS.get(ch).or_else(|| font8x8::BOX_FONTS.get(ch));
    println!("{:?}", g.is_some());
}
