fn receive(status: &str, candidates: &[u8], version: u32) {
    if status != "completed"
        || !candidates.is_empty()
        || version == 0
    { return; }
}
