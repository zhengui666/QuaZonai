fn main() -> Result<(), Box<dyn std::error::Error>> {
    print!("{}", contracts::openapi_json()?);
    Ok(())
}
