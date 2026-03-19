//! Precision Formatter - Format price and quantity to match Binance tickSize/stepSize.

use std::collections::HashMap;

/// Precision Formatter for Binance order formatting
pub struct PrecisionFormatter {
    // Symbol -> (price_precision, quantity_precision)
    precisions: HashMap<String, (u32, u32)>,
}

impl PrecisionFormatter {
    /// Create a new Precision Formatter with default precisions
    pub fn new() -> Self {
        let mut precisions = HashMap::new();
        
        // Common precisions (in production, fetch from exchangeInfo)
        precisions.insert("BTCUSDT".to_string(), (2, 5));  // 2 decimal places for price, 5 for qty
        precisions.insert("ETHUSDT".to_string(), (2, 4));
        precisions.insert("BNBUSDT".to_string(), (2, 3));
        precisions.insert("SOLUSDT".to_string(), (3, 2));
        precisions.insert("XRPUSDT".to_string(), (4, 1));
        
        Self { precisions }
    }

    /// Set precision for a symbol
    pub fn set_precision(&mut self, symbol: &str, price_precision: u32, quantity_precision: u32) {
        self.precisions.insert(symbol.to_uppercase(), (price_precision, quantity_precision));
    }

    /// Format price and quantity
    pub fn format(&self, symbol: &str, price: f64, quantity: f64) -> (f64, f64) {
        let (price_prec, qty_prec) = self.precisions.get(&symbol.to_uppercase())
            .unwrap_or(&(2, 3)); // Default precision

        let formatted_price = (price * 10f64.powi(*price_prec as i32)).round() / 10f64.powi(*price_prec as i32);
        let formatted_qty = (quantity * 10f64.powi(*qty_prec as i32)).round() / 10f64.powi(*qty_prec as i32);

        (formatted_price, formatted_qty)
    }

    /// Adjust price by 1 tick
    pub fn adjust_by_tick(&self, symbol: &str, price: f64, ticks: i32) -> f64 {
        let (price_prec, _) = self.precisions.get(&symbol.to_uppercase())
            .unwrap_or(&(2, 3));

        let tick_size = 10f64.powi(-(*price_prec as i32));
        price + (ticks as f64 * tick_size)
    }
}

impl Default for PrecisionFormatter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_btc() {
        let formatter = PrecisionFormatter::new();
        let (price, qty) = formatter.format("BTCUSDT", 95432.123456, 0.123456789);
        
        assert!((price - 95432.12).abs() < 0.01);
        assert!((qty - 0.12346).abs() < 0.00001);
    }

    #[test]
    fn test_adjust_by_tick() {
        let formatter = PrecisionFormatter::new();
        let adjusted = formatter.adjust_by_tick("BTCUSDT", 95000.00, 1);
        assert!((adjusted - 95000.01).abs() < 0.01);
    }
}
