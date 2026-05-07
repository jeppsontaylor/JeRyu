use font8x8::UnicodeFonts;
fn main() {
    let ch = '┌'; 
    let g = [
        font8x8::BASIC_FONTS.get(ch),
        font8x8::LATIN_FONTS.get(ch),
        font8x8::BLOCK_FONTS.get(ch),
        font8x8::BOX_FONTS.get(ch),
        font8x8::GREEK_FONTS.get(ch),
        font8x8::HIRAGANA_FONTS.get(ch),
    ]
    .into_iter()
    .flatten()
    .next();
    println!("{:?}", g.is_some());
}
