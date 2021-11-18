use crate::build_in_types::BuiltInType;
use crate::parsed_extern_fn::ParsedExternFn;
use quote::ToTokens;
use std::ops::Deref;
use syn::{FnArg, Pat, ReturnType};

impl ParsedExternFn {
    pub fn to_swift_param_names_and_types(&self, include_receiver_if_present: bool) -> String {
        let mut params: Vec<String> = vec![];

        for arg in &self.func.sig.inputs {
            let param = match arg {
                FnArg::Receiver(_receiver) => {
                    if include_receiver_if_present {
                        params.push(format!("_ this: UnsafeMutableRawPointer"));
                    }

                    continue;
                }
                FnArg::Typed(pat_ty) => {
                    match pat_ty.pat.deref() {
                        Pat::Ident(pat) if pat.ident.to_string() == "self" => {
                            continue;
                        }
                        _ => {}
                    };

                    let arg_name = pat_ty.pat.to_token_stream().to_string();

                    if let Some(built_in) = BuiltInType::with_type(&pat_ty.ty) {
                        format!("{}: {}", arg_name, built_in.to_swift())
                    } else {
                        // &mut Foo -> "& mut Foo"
                        let ty = pat_ty.ty.to_token_stream().to_string();
                        // Remove all references "&" and mut keywords.
                        let ty = ty.split_whitespace().last().unwrap();

                        format!("{}: {}", arg_name, ty)
                    }
                }
            };

            params.push(format!("_ {}", param))
        }

        params.join(", ")
    }

    // fn foo (&self, arg1: u8, arg2: u32)
    //  becomes..
    // ptr, arg1, arg2
    pub fn to_swift_call_args(&self, include_receiver_if_present: bool) -> String {
        let mut args = vec![];
        let inputs = &self.func.sig.inputs;
        for arg in inputs {
            match arg {
                FnArg::Receiver(_receiver) => {
                    if include_receiver_if_present {
                        args.push("ptr".to_string());
                    }
                }
                FnArg::Typed(pat_ty) => {
                    let pat = &pat_ty.pat;

                    if let Some(built_in) = BuiltInType::with_type(&pat_ty.ty) {
                        args.push(pat.to_token_stream().to_string());
                    } else {
                        args.push(format!("{}.ptr", pat.to_token_stream().to_string()));
                    };
                }
            };
        }

        args.join(", ")
    }

    pub fn to_swift_return(&self) -> String {
        match &self.func.sig.output {
            ReturnType::Default => "".to_string(),
            ReturnType::Type(_, ty) => {
                if let Some(built_in) = BuiltInType::with_type(&ty) {
                    format!(" -> {}", built_in.to_swift())
                } else {
                    format!(" -> UnsafeMutableRawPointer")
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::SwiftBridgeModuleAndErrors;
    use crate::SwiftBridgeModule;
    use proc_macro2::TokenStream;
    use quote::quote;

    /// Verify that if we are returning a declared type (non built-in) we return it as a pointer.
    #[test]
    fn return_declared_type() {
        let tokens = quote! {
            #[swift_bridge::bridge]
            mod ffi {
                extern "Rust" {
                    type Foo;
                    fn make1 () -> Foo;
                    fn make2 () -> &Foo;
                    fn make3 () -> &mut Foo;
                }
            }
        };
        let module = parse_ok(tokens);
        let functions = &module.functions;
        assert_eq!(functions.len(), 3);

        for idx in 0..3 {
            assert_eq!(
                functions[idx].to_swift_return(),
                " -> UnsafeMutableRawPointer"
            );
        }
    }

    /// Verify that we ignore self when generating Swift function params.
    #[test]
    fn excludes_self_from_params() {
        let tokens = quote! {
            #[swift_bridge::bridge]
            mod ffi {
                extern "Rust" {
                    type Foo;
                    fn make1 (self);
                    fn make2 (&self);
                    fn make3 (&mut self);
                    fn make4 (self: Foo);
                    fn make5 (self: &Foo);
                    fn make6 (self: &mut Foo);
                }
            }
        };
        let module = parse_ok(tokens);
        let methods = &module.functions;
        assert_eq!(methods.len(), 6);

        for method in methods {
            assert_eq!(method.to_swift_param_names_and_types(false), "");
        }
    }

    /// Verify that we always use the corresponding class name for an argument of a custom type.
    #[test]
    fn strips_references_from_params_with_declared_type() {
        let tokens = quote! {
            #[swift_bridge::bridge]
            mod ffi {
                extern "Rust" {
                    type Foo;
                    fn make1 (other: Foo);
                    fn make2 (other: &Foo);
                    fn make3 (other: &mut Foo);
                }
            }
        };
        let module = parse_ok(tokens);
        let functions = &module.functions;
        assert_eq!(functions.len(), 3);

        for idx in 0..3 {
            assert_eq!(
                functions[idx].to_swift_param_names_and_types(false),
                "_ other: Foo"
            );
        }
    }

    /// Verify that we use the `.ptr` field on a class instance when calling a Rust function from
    /// Swift.
    #[test]
    fn calls_args_uses_pointer_from_class_instances() {
        let tokens = quote! {
            #[swift_bridge::bridge]
            mod ffi {
                extern "Rust" {
                    type Foo;
                    fn make1 (other: Foo);
                    fn make2 (other: &Foo);
                    fn make3 (other: &mut Foo);
                }
            }
        };
        let module = parse_ok(tokens);
        let functions = &module.functions;
        assert_eq!(functions.len(), 3);

        for idx in 0..3 {
            assert_eq!(functions[idx].to_swift_call_args(true), "other.ptr");
        }
    }

    fn parse_ok(tokens: TokenStream) -> SwiftBridgeModule {
        let module_and_errors: SwiftBridgeModuleAndErrors = syn::parse2(tokens).unwrap();
        module_and_errors.module
    }
}
