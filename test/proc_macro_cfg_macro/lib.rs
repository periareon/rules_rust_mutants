use proc_macro::TokenStream;

#[proc_macro]
pub fn generate_cfg_value(_input: TokenStream) -> TokenStream {
    r#"
pub fn generated_cfg_value() -> &'static str {
    #[cfg(mutation_proc_macro_enabled)]
    {
        "enabled"
    }
    #[cfg(not(mutation_proc_macro_enabled))]
    {
        "disabled"
    }
}
"#
        .parse()
        .expect("generated proc-macro output should parse")
}
