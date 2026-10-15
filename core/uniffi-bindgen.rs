// Generates the Swift (and later Kotlin and C#) bindings from the compiled
// library, so the bindings always match the code that was actually built.
fn main() {
    uniffi::uniffi_bindgen_main()
}
