pub fn price_order(quantity: u32, unit_price: u32) -> u32 {
    quantity.saturating_mul(unit_price)
}
