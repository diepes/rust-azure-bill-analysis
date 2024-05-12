#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_csv() {
        let bills = read_csv("sample_data.csv").unwrap();
        assert_eq!(bills.len(), 2);
        // Add more assertions here based on your Bill struct fields
    }
}
