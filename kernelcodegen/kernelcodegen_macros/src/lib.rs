extern crate proc_macro;

use std::io::Write;

use proc_macro::TokenStream;
use proc_macro2::{Delimiter, Group, Literal, TokenTree};
use quote::quote;

enum BindingType {
    Storage,
    Uniform,
}

struct ShaderArg {
    binding: BindingType,
    descriptor_id: usize,
    binding_id: usize,
}

impl ShaderArg {
    fn to_bind_group_layout_entry(&self) -> Vec<proc_macro2::TokenTree> {
        assert_eq!(self.descriptor_id, 0);

        let mut tokens: Vec<proc_macro2::TokenTree> = Vec::new();
        tokens.extend(quote! { wgpu::BindGroupLayoutEntry });
        tokens.push(TokenTree::from(Group::new(Delimiter::Brace, {
            let mut tokens: Vec<proc_macro2::TokenTree> = Vec::new();
            tokens.extend(quote! { binding: });
            tokens.push(TokenTree::from(Literal::usize_unsuffixed(self.binding_id)));
            tokens.extend(quote! { , count: None, visibility: wgpu::ShaderStages::COMPUTE, });
            tokens.extend(quote! { ty: wgpu::BindingType::Buffer });
            tokens.push(TokenTree::from(Group::new(Delimiter::Brace, {
                let mut tokens: Vec<proc_macro2::TokenTree> = Vec::new();
                tokens.extend(quote! {
                    has_dynamic_offset: false,
                    min_binding_size: Some(NonZeroU64::new(1).unwrap()),
                    ty:
                });
                match self.binding {
                    BindingType::Storage => {
                        tokens.extend(
                            quote! { wgpu::BufferBindingType::Storage { read_only: false } },
                        );
                    }
                    _ => {
                        tokens.extend(quote! { wgpu::BufferBindingType::Uniform });
                    }
                }

                tokens.into_iter().collect()
            })));

            tokens.into_iter().collect()
        })));

        tokens
    }
}

fn create_bind_group_layout_args(mod_name: &str, args: &[ShaderArg]) -> TokenTree {
    let label = format!("{}_bind_group_layout", mod_name);
    let mut tokens: Vec<proc_macro2::TokenTree> = Vec::new();
    tokens.extend(quote! { &wgpu::BindGroupLayoutDescriptor });
    tokens.push(TokenTree::from(Group::new(Delimiter::Brace, {
        let mut descriptor_tokens: Vec<proc_macro2::TokenTree> = Vec::new();
        descriptor_tokens.extend(quote! { label: Some});
        descriptor_tokens.push(TokenTree::from(Group::new(
            Delimiter::Parenthesis,
            [TokenTree::from(Literal::string(&label))]
                .into_iter()
                .collect(),
        )));
        descriptor_tokens.extend(quote! {, entries: &});
        descriptor_tokens.push(TokenTree::from(Group::new(Delimiter::Bracket, {
            let mut arg_tokens: Vec<proc_macro2::TokenTree> = Vec::new();
            for arg in args.iter() {
                arg_tokens.extend(arg.to_bind_group_layout_entry());
                arg_tokens.extend(quote! {,});
            }
            arg_tokens.into_iter().collect()
        })));
        descriptor_tokens.into_iter().collect()
    })));

    TokenTree::from(Group::new(
        Delimiter::Parenthesis,
        tokens.into_iter().collect(),
    ))
}

#[proc_macro_attribute]
pub fn generate_kernel(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let item: proc_macro2::TokenStream = item.into();

    let modname = std::env::var("CARGO_PKG_NAME").unwrap();
    let mut entrypt = "main_bar".to_owned();

    let mut next_ident_is_entrypt = false;
    let mut next_group_is_arg_list = false;
    let mut args: Vec<ShaderArg> = Vec::new();

    let mut workgroup_dim: (u32, u32, u32) = (0, 0, 0);

    let mut tokens = item.clone().into_iter();
    let _punct = tokens.next();
    let attr = tokens.next().unwrap();
    match attr {
        TokenTree::Group(g) => {
            let mut tokens = g.stream().into_iter();
            let spirv_decl = tokens.next();
            match spirv_decl.unwrap() {
                TokenTree::Ident(id) => {
                    assert_eq!(id.span().source_text().unwrap(), "spirv");
                }
                _ => panic!("Unexpected token"),
            };
            let inner = tokens.next();
            match inner.unwrap() {
                TokenTree::Group(g) => {
                    let mut tokens = g.stream().into_iter();
                    let inner = tokens.next();
                    match inner.unwrap() {
                        TokenTree::Ident(id) => {
                            assert_eq!(id.span().source_text().unwrap(), "compute");
                        }
                        _ => panic!("Unexpected token"),
                    }
                    let inner = tokens.next();
                    match inner.unwrap() {
                        TokenTree::Group(g) => {
                            let mut tokens = g.stream().into_iter();
                            let threads = tokens.next();
                            match threads.unwrap() {
                                TokenTree::Ident(id) => {
                                    assert_eq!(id.span().source_text().unwrap(), "threads");
                                }
                                _ => panic!("Unexpected token"),
                            }

                            let thread_args = tokens.next();
                            match thread_args.unwrap() {
                                TokenTree::Group(g) => {
                                    let mut tokens = g.stream().into_iter();
                                    let token_to_int = |x: TokenTree| {
                                        x.span().source_text().unwrap().parse().unwrap()
                                    };
                                    let value_x = tokens.next().map(token_to_int);
                                    let _punct = tokens.next();
                                    let value_y = tokens.next().map(token_to_int);
                                    let _punct = tokens.next();
                                    let value_z = tokens.next().map(token_to_int);

                                    workgroup_dim.0 = value_x.unwrap_or(1);
                                    workgroup_dim.1 = value_y.unwrap_or(1);
                                    workgroup_dim.2 = value_z.unwrap_or(1);
                                }
                                _ => panic!("Unexpected token"),
                            }
                        }
                        _ => panic!("Unexpected token"),
                    }
                }
                _ => panic!("Unexpected token"),
            };
        }
        _ => panic!("Unexpected token"),
    };

    for tt in tokens {
        match tt {
            TokenTree::Ident(i) => {
                if let Some(id) = i.span().source_text() {
                    if next_ident_is_entrypt {
                        entrypt = id.to_owned();
                        next_ident_is_entrypt = false;
                        next_group_is_arg_list = true;
                    } else {
                        if id == "fn" {
                            next_ident_is_entrypt = true;
                        }
                    }
                }
            }
            TokenTree::Group(g) => {
                if g.delimiter() != Delimiter::Parenthesis || !next_group_is_arg_list {
                    continue;
                }

                let mut next_group_is_arg_attr = false;
                for tt in g.stream().into_iter() {
                    match tt {
                        TokenTree::Punct(p) => {
                            if let Some(c) = p.span().source_text() {
                                if c == "#" {
                                    next_group_is_arg_attr = true;
                                }
                            }
                        }
                        TokenTree::Group(g) => {
                            if !next_group_is_arg_attr {
                                continue;
                            }
                            assert!(g.delimiter() == Delimiter::Bracket);

                            // eprintln!("!!!! arg_attr {:?}", g.stream());
                            // TODO assert that _token is Ident("spriv") or Ident(rust_gpu::spriv)
                            let mut next_group_is_spirv_arg_attr = false;
                            for tt in g.stream().into_iter() {
                                match tt {
                                    TokenTree::Ident(i) => {
                                        if let Some(c) = i.span().source_text() {
                                            if c == "spirv" {
                                                next_group_is_spirv_arg_attr = true;
                                            }
                                        }
                                    }
                                    TokenTree::Group(g) => {
                                        if !next_group_is_spirv_arg_attr {
                                            continue;
                                        }
                                        next_group_is_spirv_arg_attr = false;

                                        let mut tokens = g.stream().into_iter();
                                        let storage_class = tokens.next();
                                        let _punct = tokens.next();
                                        let _descriptor_set = tokens.next();
                                        let _punct = tokens.next();
                                        let descriptor_set_id = tokens.next();
                                        let _punct = tokens.next();
                                        let _binding = tokens.next();
                                        let _punct = tokens.next();
                                        let binding_id = tokens.next();

                                        if let Some(TokenTree::Ident(ref storage_class)) =
                                            storage_class
                                        {
                                            let s = storage_class.span().source_text().unwrap();
                                            if s == "global_invocation_id" {
                                                continue;
                                            }

                                            let binding = if s == "storage_buffer" {
                                                BindingType::Storage
                                            } else {
                                                BindingType::Uniform
                                            };

                                            let descriptor_id = match descriptor_set_id.unwrap() {
                                                TokenTree::Literal(d) => {
                                                    d.span().source_text().unwrap().parse().unwrap()
                                                }
                                                _ => 0,
                                            };

                                            let binding_id = match binding_id.unwrap() {
                                                TokenTree::Literal(d) => {
                                                    d.span().source_text().unwrap().parse().unwrap()
                                                }
                                                _ => 0,
                                            };

                                            args.push(ShaderArg {
                                                binding,
                                                descriptor_id,
                                                binding_id,
                                            });
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            next_group_is_arg_attr = false;
                        }
                        _ => {}
                    }
                }
                next_group_is_arg_list = false;
            }
            _ => {}
        }
    }

    let mut tokens: Vec<proc_macro2::TokenTree> = Vec::new();
    // Module header
    tokens.extend(quote! {
        #[cfg(not(target_arch = "spirv"))]
        pub mod codegen
    });

    // Generate module body
    let mut modtokens: Vec<proc_macro2::TokenTree> = Vec::new();
    modtokens.extend(quote! {
        use kernelcodegen::{ComputeKernel, wgpu};
        use wgpu::Device;
        use core::num::NonZeroU64;

        pub fn new(device: &Device, shader_bytes: &[u8]) -> ComputeKernel
    });

    let mut fntokens: Vec<proc_macro2::TokenTree> = Vec::new();
    fntokens.extend(quote! { let bind_group_layout = device.create_bind_group_layout });
    fntokens.push(create_bind_group_layout_args(&modname, &args));
    fntokens.extend(quote! {;});

    fntokens.extend(quote! { let pipeline_layout = device.create_pipeline_layout });
    fntokens.push(TokenTree::from(Group::new(Delimiter::Parenthesis, {
        let mut create_pipeline_tokens: Vec<proc_macro2::TokenTree> = Vec::new();
        create_pipeline_tokens.extend(quote! {&wgpu::PipelineLayoutDescriptor});
        create_pipeline_tokens.push(TokenTree::from(Group::new(Delimiter::Brace, {
            let mut create_pipeline_fields_tokens: Vec<proc_macro2::TokenTree> = Vec::new();
            create_pipeline_fields_tokens.extend(quote! {label: Some});
            create_pipeline_fields_tokens.push(TokenTree::from(Group::new(
                Delimiter::Parenthesis,
                [TokenTree::from(Literal::string(&format!(
                    "{}_layout",
                    modname
                )))]
                .into_iter()
                .collect(),
            )));
            create_pipeline_fields_tokens.extend(quote! {
               ,
               bind_group_layouts: &[&bind_group_layout],
               push_constant_ranges: &[],
            });
            create_pipeline_fields_tokens.into_iter().collect()
        })));
        create_pipeline_tokens.into_iter().collect()
    })));
    fntokens.extend(quote! {;});

    fntokens.extend(quote! {
      let spirv = std::borrow::Cow::Owned(wgpu::util::make_spirv_raw(shader_bytes).into_owned());
      let shader_binary = wgpu::ShaderModuleDescriptorSpirV
    });
    fntokens.push(TokenTree::from(Group::new(Delimiter::Brace, {
        let mut shader_desc_tokens: Vec<proc_macro2::TokenTree> = Vec::new();
        shader_desc_tokens.extend(quote! {label: Some});
        shader_desc_tokens.push(TokenTree::from(Group::new(
            Delimiter::Parenthesis,
            [TokenTree::from(Literal::string(&modname))]
                .into_iter()
                .collect(),
        )));
        shader_desc_tokens.extend(quote! {, source: spirv});
        shader_desc_tokens.into_iter().collect()
    })));
    fntokens.extend(quote! {;});
    fntokens.extend(
        quote! { let module = unsafe { device.create_shader_module_spirv(&shader_binary) }; },
    );

    fntokens.extend(quote! { let compute_pipeline  = device.create_compute_pipeline });
    fntokens.push(TokenTree::from(Group::new(Delimiter::Parenthesis, {
        let mut create_pipeline_tokens: Vec<proc_macro2::TokenTree> = Vec::new();
        create_pipeline_tokens.extend(quote! {&wgpu::ComputePipelineDescriptor});
        create_pipeline_tokens.push(TokenTree::from(Group::new(Delimiter::Brace, {
            let mut create_pipeline_fields_tokens: Vec<proc_macro2::TokenTree> = Vec::new();
            create_pipeline_fields_tokens.extend(quote! {label: Some});
            create_pipeline_fields_tokens.push(TokenTree::from(Group::new(
                Delimiter::Parenthesis,
                [TokenTree::from(Literal::string(&format!(
                    "{}_compute_pipeline",
                    modname
                )))]
                .into_iter()
                .collect(),
            )));
            create_pipeline_fields_tokens.extend(quote! {
               ,
               layout: Some(&pipeline_layout),
               module: &module,
               entry_point: Some
            });
            create_pipeline_fields_tokens.push(TokenTree::from(Group::new(
                Delimiter::Parenthesis,
                [TokenTree::from(Literal::string(&entrypt))]
                    .into_iter()
                    .collect(),
            )));
            create_pipeline_fields_tokens.extend(quote! {
               ,
               compilation_options: Default::default(),
               cache: None
            });
            create_pipeline_fields_tokens.into_iter().collect()
        })));
        create_pipeline_tokens.into_iter().collect()
    })));
    fntokens.extend(quote! {;});

    fntokens.extend(quote! { let workgroup_dim = });
    fntokens.push(TokenTree::from(Group::new(Delimiter::Parenthesis, {
        let mut tuple_tokens: Vec<proc_macro2::TokenTree> = Vec::new();
        tuple_tokens.push(TokenTree::from(Literal::u32_unsuffixed(workgroup_dim.0)));
        tuple_tokens.extend(quote! {,});
        tuple_tokens.push(TokenTree::from(Literal::u32_unsuffixed(workgroup_dim.1)));
        tuple_tokens.extend(quote! {,});
        tuple_tokens.push(TokenTree::from(Literal::u32_unsuffixed(workgroup_dim.2)));
        tuple_tokens.into_iter().collect()
    })));
    fntokens.extend(quote! {
        ;
        ComputeKernel {
            bind_group_layout,
            pipeline_layout,
            compute_pipeline,
            workgroup_dim,
        }
    });

    // Add function body
    modtokens.push(TokenTree::from(Group::new(
        Delimiter::Brace,
        fntokens.into_iter().collect(),
    )));

    // Add module body
    tokens.push(TokenTree::from(Group::new(
        Delimiter::Brace,
        modtokens.into_iter().collect(),
    )));

    let mut res = proc_macro2::TokenStream::new();
    res.extend(tokens.into_iter().collect::<proc_macro2::TokenStream>());
    res.extend(item.clone());
    let res = res.into();

    if let Ok(out_dir) = std::env::var("OUT_DIR") {
        let path = std::path::PathBuf::from(out_dir);
        let path = path.join("generate_kernel.out.txt");

        let mut f = std::fs::File::create(path).unwrap();
        let _ = f.write_all(format!("generated code: {}\n", res).as_bytes());
        let _ = f.write_all(format!("tokens: {:?}\n", res).as_bytes());
    }

    res
}
