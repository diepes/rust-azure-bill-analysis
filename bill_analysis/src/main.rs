use bill_analysis::bill::Bill;

fn main() {
    println!("Hello, world!!");
    let csv_file_name = "../Detail_Enrollment_70785102_202404_en.csv";
    // let csv_file_name = "tests/azure_test_data_01.csv";
    let bills = Bill::parse_csv(&csv_file_name).unwrap();
    println!("{:?}", bills.len());
    println!("{:?}", bills[0]);
}
