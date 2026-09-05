fn main() -> Result<(), Box<dyn std::error::Error>> {
    print!("{}", qz_contracts::openapi_json()?);
    Ok(())
}
