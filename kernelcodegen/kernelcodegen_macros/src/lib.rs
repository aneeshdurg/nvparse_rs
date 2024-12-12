extern crate proc_macro;

use std::io::Write;

use proc_macro::TokenStream;
use proc_macro2::{Delimiter, Group, Ident, Literal, Span, TokenTree};
use quote::quote;

struct ShaderArg {}

impl ShaderArg {
    fn to_bind_group_layout_entry(&self) -> proc_macro2::TokenTree {
        unimplemented!()
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
            for arg in args {
                arg_tokens.push(arg.to_bind_group_layout_entry());
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
pub fn myattr(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let item: proc_macro2::TokenStream = item.into();
    // TODO parse the following from item
    let args: Vec<ShaderArg> = Vec::new();
    let modname = "foo";
    let entrypt = "main_bar";
    let workgroup_dim: (u32, u32, u32) = (0, 0, 0);

    let mut tokens: Vec<proc_macro2::TokenTree> = Vec::new();
    // Module header
    tokens.extend(quote! {
        #[cfg(not(target_arch = "spirv"))]
        pub mod codegen
    });

    // Generate module body
    let mut modtokens: Vec<proc_macro2::TokenTree> = Vec::new();
    modtokens.extend(quote! {
        use kernelcodegen_types::Generated;
        use wgpu::Device;
        use crate::glam::UVec3;

        pub fn new(device: &Device, shader_bytes: &[u8]) -> Generated
    });

    let mut fntokens: Vec<proc_macro2::TokenTree> = Vec::new();
    fntokens.extend(quote! { let bind_group_layout = device.create_bind_group_layout });
    fntokens.push(create_bind_group_layout_args(modname, &args));
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
            [TokenTree::from(Literal::string(modname))]
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
                [TokenTree::from(Literal::string(entrypt))]
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

    fntokens.extend(quote! { let workgroup_dim = UVec3::from });
    fntokens.push(TokenTree::from(Group::new(
        Delimiter::Parenthesis,
        [TokenTree::from(Group::new(Delimiter::Parenthesis, {
            let mut tuple_tokens: Vec<proc_macro2::TokenTree> = Vec::new();
            tuple_tokens.push(TokenTree::from(Literal::u32_unsuffixed(workgroup_dim.0)));
            tuple_tokens.extend(quote! {,});
            tuple_tokens.push(TokenTree::from(Literal::u32_unsuffixed(workgroup_dim.1)));
            tuple_tokens.extend(quote! {,});
            tuple_tokens.push(TokenTree::from(Literal::u32_unsuffixed(workgroup_dim.2)));
            tuple_tokens.into_iter().collect()
        }))]
        .into_iter()
        .collect(),
    )));
    fntokens.extend(quote! {
        ;
        Generated {
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
    res.extend(item);
    let res = res.into();

    if let Ok(out_dir) = std::env::var("OUT_DIR") {
        let path = std::path::PathBuf::from(out_dir);
        let path = path.join("myattr.out");

        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(format!("generated code: {}\n", res).as_bytes())
            .expect("Write to output failed");
        f.write_all(format!("tokens: {:?}\n", res).as_bytes())
            .expect("Write to output failed");
    }

    res
}
