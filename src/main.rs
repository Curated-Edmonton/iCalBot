use dotenv;

fn main() {
    // Load environment variables from a .env file, if it exists
    dotenv::dotenv().ok();

    println!("Hello, world!");
}
