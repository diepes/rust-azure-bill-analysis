use bill_analysis::bill::Bill;

fn main() {
    println!("Hello, world!!");
    let bills = Bill::parse_csv("tests/azure_test_data.csv").unwrap();
    println!("{:?}", bills);
}
