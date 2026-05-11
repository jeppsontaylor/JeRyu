use font8x8::UnicodeFonts;
fn main() {
    let ch = '┌'; // Box drawing character
    let g = font8x8::BOX_FONTS.get(ch);
    println!("{:?}", g.is_some());
}
