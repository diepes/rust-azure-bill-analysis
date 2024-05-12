use bill_analysis::Bill;

fn main() {
    println!("Hello, world!!");
    let bills = bill_analysis::Bill::parse_csv("tests/azure_test_data.csv").unwrap();
    println!("{:?}", bills);
}
