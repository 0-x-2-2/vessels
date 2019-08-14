use crate::{prefix, proc_macro::TokenStream};
use proc_macro2::Span;
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};
use syn::spanned::Spanned;
use syn::{
    parse_macro_input, punctuated::Punctuated, token::Paren, Field, Fields, FieldsUnnamed, FnArg,
    Ident, ItemTrait, Path, PathArguments, PathSegment, ReturnType, TraitBound, TraitBoundModifier,
    TraitItem, TraitItemMethod, Type, TypeParamBound, TypeVerbatim, Variant, Visibility,
};
use synstructure::ToTokens;

#[derive(Debug, Hash)]
pub(crate) struct Procedure {
    pub arg_types: Vec<Type>,
    pub mut_receiver: bool,
    pub ident: Option<Ident>,
    pub(crate) return_type: Option<Type>,
}

pub(crate) fn generate_enum(methods: &[Procedure]) -> Vec<Variant> {
    methods
        .iter()
        .map(|method| Variant {
            ident: method.ident.clone().unwrap(),
            attrs: vec![],
            discriminant: None,
            fields: {
                let mut fields = Punctuated::new();
                for ty in &method.arg_types {
                    fields.push(Field {
                        attrs: vec![],
                        ident: None,
                        ty: ty.clone(),
                        colon_token: None,
                        vis: Visibility::Inherited,
                    });
                }
                fields.push(Field {
                    attrs: vec![],
                    ident: None,
                    ty: Type::Verbatim(TypeVerbatim {
                        tts: quote! {
                            u64
                        },
                    }),
                    colon_token: None,
                    vis: Visibility::Inherited,
                });
                Fields::Unnamed(FieldsUnnamed {
                    paren_token: Paren(Span::call_site()),
                    unnamed: fields,
                })
            },
        })
        .collect::<Vec<_>>()
}

fn generate_return_variants(methods: &[Procedure]) -> Vec<Variant> {
    methods
        .iter()
        .map(|method| Variant {
            ident: method.ident.clone().unwrap(),
            attrs: vec![],
            discriminant: None,
            fields: {
                let mut fields = Punctuated::new();
                let ty = &method.return_type;
                fields.push(Field {
                    attrs: vec![],
                    ident: None,
                    ty: Type::Verbatim(TypeVerbatim {
                        tts: quote! {
                            <#ty as ::vessels::protocol::Value>::Item
                        },
                    }),
                    colon_token: None,
                    vis: Visibility::Inherited,
                });
                fields.push(Field {
                    attrs: vec![],
                    ident: None,
                    ty: Type::Verbatim(TypeVerbatim {
                        tts: quote! {
                            u64
                        },
                    }),
                    colon_token: None,
                    vis: Visibility::Inherited,
                });
                fields.push(Field {
                    attrs: vec![],
                    ident: None,
                    ty: Type::Verbatim(TypeVerbatim {
                        tts: quote! {
                            u64
                        },
                    }),
                    colon_token: None,
                    vis: Visibility::Inherited,
                });
                Fields::Unnamed(FieldsUnnamed {
                    paren_token: Paren(Span::call_site()),
                    unnamed: fields,
                })
            },
        })
        .collect::<Vec<_>>()
}

pub(crate) fn generate_remote_impl(
    ident: &Ident,
    methods: &[Procedure],
) -> proc_macro2::TokenStream {
    let call_inner = prefix(ident, "Call_Inner");
    let call = prefix(ident, "Call");
    let channel = prefix(ident, "Channel");
    let mut stream = proc_macro2::TokenStream::new();
    for method in methods.iter() {
        let index_ident = method.ident.clone().unwrap();
        let ident = &method.ident;
        let mut arg_stream = proc_macro2::TokenStream::new();
        let mut arg_names_stream = proc_macro2::TokenStream::new();
        if method.mut_receiver {
            arg_stream.extend(quote! {
                &mut self,
            });
        } else {
            arg_stream.extend(quote! {
                &self,
            });
        }
        let mut call_sig = proc_macro2::TokenStream::new();
        for (index, ty) in method.arg_types.iter().enumerate() {
            let ident = Ident::new(&format!("_{}", index), Span::call_site());
            arg_stream.extend(quote! {
                #ident: #ty,
            });
            arg_names_stream.extend(quote! {
                #ident,
            });
        }
        arg_names_stream.extend(quote! {
            _proto_id,
        });
        call_sig.extend(quote! {
            (#arg_names_stream)
        });
        let return_type = &method.return_type;
        stream.extend(quote! {
            fn #ident(#arg_stream) -> #return_type {
                let _proto_id = self.next_id();
                let (ct, ct1) = ::vessels::protocol::Context::new();
                self.channels.write().unwrap().insert(_proto_id, #channel::#ident(Box::new(ct1)));
                self.queue.write().unwrap().push_back(#call {call: #call_inner::#index_ident#call_sig});
                self.task.notify();
                <#return_type as ::vessels::protocol::Value>::construct(ct)
            }
        });
    }
    stream
}

pub(crate) fn generate_serialize_impl(
    ident: &Ident,
    methods: &[Procedure],
) -> proc_macro2::TokenStream {
    let call_inner = prefix(ident, "Call_Inner");
    let mut arms = proc_macro2::TokenStream::new();
    for (index, method) in methods.iter().enumerate() {
        let ident = &method.ident;
        let mut sig = proc_macro2::TokenStream::new();
        let mut args = proc_macro2::TokenStream::new();
        let mut element_calls = proc_macro2::TokenStream::new();
        let t_len = method.arg_types.len() + 2;
        for index in 0..=method.arg_types.len() {
            let ident = Ident::new(&format!("_{}", index), Span::call_site());
            args.extend(quote! {
                #ident,
            });
            element_calls.extend(quote! {
                seq.serialize_element(#ident)?;
            });
        }
        sig.extend(quote! {
            (#args)
        });
        arms.extend(quote! {
            #call_inner::#ident#sig => {
                let mut seq = serializer.serialize_seq(Some(#t_len))?;
                seq.serialize_element(&#index)?;
                #element_calls
                seq.end()
            },
        });
    }
    arms
}

pub(crate) fn generate_serialize_return_impl(
    ident: &Ident,
    methods: &[Procedure],
) -> proc_macro2::TokenStream {
    let response = prefix(ident, "Response");
    let mut arms = proc_macro2::TokenStream::new();
    for method in methods {
        let ident = &method.ident;
        arms.extend(quote! {
            #response::#ident(data, idx, m) => {
                let mut seq = serializer.serialize_seq(Some(3))?;
                seq.serialize_element(m)?;
                seq.serialize_element(idx)?;
                seq.serialize_element(data)?;
                seq.end()
            },
        });
    }
    arms
}

pub(crate) fn generate_deserialize_impl(
    ident: &Ident,
    methods: &[Procedure],
) -> proc_macro2::TokenStream {
    let call_inner = prefix(ident, "Call_Inner");
    let call = prefix(ident, "Call");
    let response_variant = prefix(ident, "Call_Response_Variant");
    let response = prefix(ident, "Response");
    let mut arms = proc_macro2::TokenStream::new();
    for (index, method) in methods.iter().enumerate() {
        let ident = &method.ident;
        let mut sig = proc_macro2::TokenStream::new();
        let mut args = proc_macro2::TokenStream::new();
        for index in (0..=method.arg_types.len()).map(|i| i + 1) {
            args.extend(quote! {
                seq.next_element()?.ok_or_else(|| ::serde::de::Error::invalid_length(#index, &self))?,
            });
        }
        sig.extend(quote! {
            (#args)
        });
        arms.extend(quote! {
            #index => {
                #call_inner::#ident#sig
            }
        });
    }
    quote! {
        Ok(#call{
            call: match index {
                #arms,
                _ => {
                    let d: #response = seq.next_element()?.ok_or_else(|| ::serde::de::Error::invalid_length(1, &self))?;
                    #call_inner::#response_variant(d)
                }
            }
        })
    }
}

pub(crate) fn generate_deserialize_return_impl(
    ident: &Ident,
    methods: &[Procedure],
) -> proc_macro2::TokenStream {
    let response = prefix(ident, "Response");
    let mut arms = proc_macro2::TokenStream::new();
    for (index, method) in methods.iter().enumerate() {
        let ident = &method.ident;
        let index = index as u64;
        arms.extend(quote! {
            #index => {
                Ok(#response::#ident(seq.next_element()?.ok_or_else(|| ::serde::de::Error::invalid_length(0, &self))?, seq.next_element()?.ok_or_else(|| ::serde::de::Error::invalid_length(0, &self))?, index))
            }
        });
    }
    quote! {
        match index {
            #arms
            _ => Err(::serde::de::Error::invalid_length(0, &self))?
        }
    }
}

pub(crate) fn generate_shim_forward(methods: &[Procedure]) -> proc_macro2::TokenStream {
    let mut calls = proc_macro2::TokenStream::new();
    for method in methods {
        let ident = &method.ident;
        let mut args = proc_macro2::TokenStream::new();
        let mut arg_names = proc_macro2::TokenStream::new();
        if !method.arg_types.is_empty() {
            for (index, ty) in method.arg_types.iter().enumerate() {
                let ident = Ident::new(&format!("_{}", index), Span::call_site());
                args.extend(quote! {
                    #ident: #ty,
                });
                arg_names.extend(quote! {
                    #ident,
                });
            }
        }
        let receiver = if method.mut_receiver {
            quote! {
                &mut self
            }
        } else {
            quote! {
                &self
            }
        };
        let return_type = &method.return_type;
        calls.extend(quote! {
            fn #ident(#receiver, #args) -> #return_type {
                let ctx = ::vessels::protocol::Context::<<#return_type as ::vessels::protocol::Value>::Item>::new();
                self.inner.#ident(#arg_names)
            }
        });
    }
    calls
}

pub(crate) fn generate_st_traits(ident: &Ident, methods: &[Procedure]) -> proc_macro2::TokenStream {
    let channel = prefix(ident, "Channel");
    let mut items = proc_macro2::TokenStream::new();
    let mut variants = proc_macro2::TokenStream::new();

    methods.iter().for_each(|m| {
        let r_type = m.return_type.as_ref().unwrap();
        let ident = prefix(ident, &format!("METHOD_TRAIT_{}", m.ident.as_ref().unwrap().to_string()));
        items.extend(quote! {
            #[allow(non_camel_case_types)]
            #[doc(hidden)]
            pub trait #ident: ::futures::Stream<Item = <#r_type as ::vessels::protocol::Value>::Item, Error = ()> + ::futures::Sink<SinkItem = <#r_type as ::vessels::protocol::Value>::Item, SinkError = ()> + Send + Sync {}
            impl<T> #ident for T where T: ::futures::Stream<Item = <#r_type as ::vessels::protocol::Value>::Item, Error = ()> + ::futures::Sink<SinkItem = <#r_type as ::vessels::protocol::Value>::Item, SinkError = ()> + Send + Sync {}
        });
        let o_ident = m.ident.as_ref().unwrap();
        variants.extend(quote! {
            #o_ident(Box<dyn #ident>),
        })
    });

    quote! {
        #[doc(hidden)]
        #[allow(non_camel_case_types)]
        pub enum #channel {
            #variants
        }
        #items
    }
}

pub(crate) fn generate_handle_response(
    ident: &Ident,
    methods: &[Procedure],
) -> proc_macro2::TokenStream {
    let channel = prefix(ident, "Channel");
    let response = prefix(ident, "Response");
    let mut arms = proc_macro2::TokenStream::new();
    for method in methods {
        let ident = method.ident.as_ref().unwrap();
        arms.extend(quote! {
            #response::#ident(data, index, id) => {
                let mut channels = self.channels.write().unwrap();
                if let Some(#channel::#ident(channel)) = channels.get_mut(&id) {
                    channel.start_send(data).unwrap();
                }
            }
        });
    }
    quote! {
        match item {
            #arms
        }
    }
}

pub(crate) fn generate_binds(ident: &Ident, methods: &[Procedure]) -> TokenStream {
    let enum_variants = generate_enum(methods);
    let return_variants = generate_return_variants(methods);
    let remote_impl = generate_remote_impl(ident, methods);
    let serialize_impl = generate_serialize_impl(ident, methods);
    let serialize_return_impl = generate_serialize_return_impl(ident, methods);
    let deserialize_impl = generate_deserialize_impl(ident, methods);
    let deserialize_return_impl = generate_deserialize_return_impl(ident, methods);
    let blanket = generate_blanket(ident, methods);
    let st_traits = generate_st_traits(ident, methods);
    let handle_response = generate_handle_response(ident, methods);
    let shim_forward = generate_shim_forward(methods);
    let call_repr: proc_macro2::TokenStream;
    let m_len = methods.len();
    let c_remote = prefix(ident, "Concrete_Remote");
    let never_ready = prefix(ident, "Never_Ready");
    let call_inner = prefix(ident, "Call_Inner");
    let protocol_shim = prefix(ident, "Protocol_Shim");
    let protocol_trait = prefix(ident, "Protocol_Trait");
    let call = prefix(ident, "Call");
    let remote = prefix(ident, "Remote");
    let response = prefix(ident, "Response");
    let response_variant = prefix(ident, "Call_Response_Variant");
    let channel = prefix(ident, "Channel");
    if methods.len() == 1 && methods[0].arg_types.is_empty() {
        call_repr = proc_macro2::TokenStream::new();
    } else {
        call_repr = quote! {
            #[repr(transparent)]
        };
    }
    let gen = quote! {
        #[allow(non_snake_case)]
        #[allow(non_camel_case_types)]
        #[derive(Clone)]
        #[allow(non_camel_case_types)]
        struct #c_remote {
            task: ::std::sync::Arc<::futures::task::AtomicTask>,
            queue: ::std::sync::Arc<::std::sync::RwLock<::std::collections::VecDeque<#call>>>,
            ids: ::std::sync::Arc<::std::sync::RwLock<Vec<u64>>>,
            last_id: ::std::sync::Arc<::std::sync::atomic::AtomicU64>,
            channels: ::std::sync::Arc<::std::sync::RwLock<::std::collections::HashMap<u64, #channel>>>,
        }
        impl #c_remote {
            pub fn new() -> #c_remote {
                #c_remote {
                    task: ::std::sync::Arc::new(::futures::task::AtomicTask::new()),
                    queue: ::std::sync::Arc::new(::std::sync::RwLock::new(::std::collections::VecDeque::new())),
                    ids: ::std::sync::Arc::new(::std::sync::RwLock::new(vec![])),
                    last_id: ::std::sync::Arc::new(::std::sync::atomic::AtomicU64::new(0)),
                    channels: ::std::sync::Arc::new(::std::sync::RwLock::new(::std::collections::HashMap::new())),
                }
            }
            fn next_id(&self) -> u64 {
                let mut ids = self.ids.write().unwrap();
                if let Some(id) = ids.pop() {
                    id
                } else {
                    self.last_id.fetch_add(1, ::std::sync::atomic::Ordering::SeqCst)
                }
            }
        }
        impl #ident for #c_remote {
            #remote_impl
        }
        impl ::futures::Stream for #c_remote {
            type Item = #call;
            type Error = ();

            fn poll(&mut self) -> ::futures::Poll<::std::option::Option<Self::Item>, Self::Error> {
                match self.queue.write().unwrap().pop_front() {
                    Some(item) => {
                        Ok(::futures::Async::Ready(Some(item)))
                    },
                    None => {
                        self.task.register();
                        Ok(::futures::Async::NotReady)
                    }
                }
            }
        }
        impl ::futures::Sink for #c_remote {
            type SinkItem = #response;
            type SinkError = ();

            fn start_send(&mut self, item: Self::SinkItem) -> ::futures::StartSend<Self::SinkItem, Self::SinkError> {
                #handle_response
                Ok(::futures::AsyncSink::Ready)
            }
            fn poll_complete(&mut self) -> ::futures::Poll<(), Self::SinkError> {
                Ok(::futures::Async::Ready(()))
            }
        }
        struct #never_ready<T, E> {
            item: ::std::marker::PhantomData<T>,
            error: ::std::marker::PhantomData<E>
        }
        impl<T, E> #never_ready<T, E> {
            fn new() -> Self {
                #never_ready {
                    item: ::std::marker::PhantomData,
                    error: ::std::marker::PhantomData,
                }
            }
        }
        impl<T, E> ::futures::Stream for #never_ready<T, E> {
            type Item = T;
            type Error = E;

            fn poll(&mut self) -> ::futures::Poll<Option<Self::Item>, Self::Error> {
                Ok(::futures::Async::NotReady)
            }
        }
        #[doc(hidden)]
        #[allow(non_camel_case_types)]
        #call_repr
        pub struct #call {
            call: #call_inner,
        }
        #[allow(non_camel_case_types)]
        enum #call_inner {
            #(#enum_variants),*,
            #response_variant(#response)
        }
        #st_traits
        #[allow(non_camel_case_types)]
        #[doc(hidden)]
        pub enum #response {
            #(#return_variants),*
        }
        impl ::serde::Serialize for #call {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where S: ::serde::Serializer {
                use ::serde::ser::SerializeSeq;
                match &self.call {
                    #serialize_impl
                    #call_inner::#response_variant(response) => {
                        let mut seq = serializer.serialize_seq(Some(4))?;
                        seq.serialize_element(&#m_len)?;
                        seq.serialize_element(response)?;
                        seq.end()
                    }
                }
            }
        }
        impl ::serde::Serialize for #response {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where S: ::serde::Serializer {
                use ::serde::ser::SerializeSeq;
                match self {
                    #serialize_return_impl
                }
            }
        }
        impl<'de> ::serde::Deserialize<'de> for #call {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error> where D: ::serde::Deserializer<'de> {
                struct CallVisitor;
                impl<'de> ::serde::de::Visitor<'de> for CallVisitor {
                    type Value = #call;

                    fn expecting(&self, formatter: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                        formatter.write_str("a serialized protocol #call")
                    }
                    fn visit_seq<V>(self, mut seq: V) -> Result<#call, V::Error> where V: ::serde::de::SeqAccess<'de>, {
                        let index: usize = seq.next_element()?.ok_or_else(|| ::serde::de::Error::invalid_length(0, &self))?;
                        #deserialize_impl
                    }
                }
                deserializer.deserialize_seq(CallVisitor)
            }
        }
        #[doc(hidden)]
        pub trait #remote: futures::Stream<Item = #call, Error = ()> + futures::Sink<SinkItem = #response, SinkError = ()> + #ident + Send {
            fn box_clone(&self) -> Box<dyn #remote>;
            fn proto_clone(&self) -> Box<dyn #ident>;
        }
        impl #remote for #c_remote {
            fn box_clone(&self) -> Box<dyn #remote> {
                Box::new(self.clone())
            }
            fn proto_clone(&self) -> Box<dyn #ident> {
                Box::new(self.clone())
            }
        }
        impl ::vessels::protocol::Remote<dyn #ident> for Box<dyn #remote> {
            fn separate(self) -> (Box<dyn #ident>, Box<dyn ::vessels::protocol::RemoteSinkStream<dyn #ident>>) {
                (self.proto_clone(), Box::new(self))
            }
        }
        impl ::vessels::protocol::RemoteSinkStream<dyn #ident> for Box<dyn #remote> {}
        impl<'de> ::serde::Deserialize<'de> for #response {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error> where D: ::serde::Deserializer<'de> {
                struct ResponseVisitor;
                impl<'de> ::serde::de::Visitor<'de> for ResponseVisitor {
                    type Value = #response;

                    fn expecting(&self, formatter: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                        formatter.write_str("a serialized protocol #response")
                    }
                    fn visit_seq<V>(self, mut seq: V) -> Result<#response, V::Error> where V: ::serde::de::SeqAccess<'de>, {
                        let index: u64 = seq.next_element()?.ok_or_else(|| ::serde::de::Error::invalid_length(0, &self))?;
                        #deserialize_return_impl
                    }
                }
                deserializer.deserialize_seq(ResponseVisitor)
            }
        }
        #[allow(non_camel_case_types)]
        struct #protocol_shim<T: #ident> {
            inner: T,
            channels: ::std::collections::HashMap<u64, #channel>,
            inner_stream: Box<dyn ::futures::Stream<Item = #response, Error = ()> + Send>,
            task: ::std::sync::Arc<::futures::task::AtomicTask>
        }
        impl<T: #ident> #protocol_shim<T> {
            pub fn new(inner: T) -> Self {
                #protocol_shim {
                    inner,
                    channels: ::std::collections::HashMap::new(),
                    inner_stream: Box::new(#never_ready::new()),
                    task: ::std::sync::Arc::new(::futures::task::AtomicTask::new())
                }
            }
        }
        impl<T> ::futures::Sink for #protocol_shim<T> where T: #ident {
            type SinkItem = #call;
            type SinkError = ();
            fn start_send(&mut self, item: Self::SinkItem) -> ::futures::StartSend<Self::SinkItem, Self::SinkError> {
                use ::vessels::protocol::Value;
                use ::futures::{Stream, Sink, Future};
                match item.call {
                    #blanket
                    #call_inner::#response_variant(resp) => {
                        // TODO
                    }
                }
                Ok(::futures::AsyncSink::Ready)
            }
            fn poll_complete(&mut self) -> ::futures::Poll<(), Self::SinkError> {
                Ok(::futures::Async::Ready(()))
            }
        }
        impl<T> ::futures::Stream for #protocol_shim<T> where T: #ident {
            type Item = #response;
            type Error = ();

            fn poll(&mut self) -> ::futures::Poll<Option<Self::Item>, Self::Error> {
                let poll = self.inner_stream.poll();
                if let Ok(::futures::Async::NotReady) = poll {
                    self.task.register();
                }
                poll
            }
        }
        pub trait #protocol_trait: ::futures::Sink<SinkItem = #call, SinkError = ()> + ::futures::Stream<Item = #response, Error = ()> + #ident + Send {}
        #[allow(non_camel_case_types)]
        impl<T> #protocol_trait for #protocol_shim<T> where T: #ident + Send {}
        impl<T: #ident> #ident for #protocol_shim<T> {
            #shim_forward
        }
    };
    gen.into()
}

pub(crate) fn generate_blanket(ident: &Ident, methods: &[Procedure]) -> proc_macro2::TokenStream {
    let call_inner = prefix(ident, "Call_Inner");
    let response = prefix(ident, "Response");
    let mut arms = proc_macro2::TokenStream::new();
    for (index, method) in methods.iter().enumerate() {
        let index = index as u64;
        let ident = &method.ident;
        let mut sig = proc_macro2::TokenStream::new();
        let mut args = proc_macro2::TokenStream::new();
        for index in 0..method.arg_types.len() {
            let ident = Ident::new(&format!("_{}", index), Span::call_site());
            args.extend(quote! {
                #ident,
            });
        }
        let mut s_args = args.clone();
        let id = Ident::new(&format!("_{}", method.arg_types.len()), Span::call_site());
        s_args.extend(quote! {
            #id,
        });
        sig.extend(quote! {
            (#s_args)
        });
        arms.extend(quote! {
            #call_inner::#ident#sig => {
                let (context, loc_context) = ::vessels::protocol::Context::new();
                self.#ident(#args).deconstruct(context);
                let (sink, stream) = loc_context.split();
                let mut i_stream: Box<dyn ::futures::Stream<Error = (), Item = #response> + Send + 'static> = Box::new(futures::stream::empty());
                std::mem::swap(&mut self.inner_stream, &mut i_stream);
                self.inner_stream = Box::new(stream.map(move |i| #response::#ident(i, #index, #id)).select(i_stream));
                self.task.notify();
            }
        });
    }
    arms
}

pub fn protocol(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return r#"compile_error!("unexpected arguments passed to `protocol`");"#
            .parse()
            .unwrap();
    }
    let mut input = {
        let item = item.clone();
        parse_macro_input!(item as ItemTrait)
    };
    if !input.generics.params.is_empty() {
        return TokenStream::from(quote_spanned! {
            input.generics.params.first().unwrap().span() =>
            compile_error!("generic parameters not allowed in `protocol` trait");
        });
    }
    if !input.supertraits.is_empty() {
        return TokenStream::from(quote_spanned! {
            input.supertraits.first().unwrap().span() =>
            compile_error!("supertraits not allowed on `protocol` trait");
        });
    }
    let mut assert_stream = TokenStream::new();
    let mut procedures = vec![];
    for (index, item) in input.items.iter_mut().enumerate() {
        let mut procedure = Procedure {
            arg_types: vec![],
            return_type: None,
            ident: None,
            mut_receiver: false,
        };
        if let TraitItem::Method(method) = item {
            if &format!("{}", method.sig.ident) == "remote" {
                return TokenStream::from(quote_spanned! {
                    method.sig.ident.span() =>
                    compile_error!("`protocol` methods must not be named remote");
                });
            }
            if &format!("{}", method.sig.ident) == "into_protocol" {
                return TokenStream::from(quote_spanned! {
                    method.sig.ident.span() =>
                    compile_error!("`protocol` methods must not be named into_protocol");
                });
            }
            if let Some(default) = &method.default {
                return TokenStream::from(quote_spanned! {
                    default.span() =>
                    compile_error!("default implementations not allowed in `protocol` methods");
                });
            }
            if !method.sig.decl.generics.params.is_empty() {
                return TokenStream::from(quote_spanned! {
                    method.sig.decl.generics.params.first().unwrap().span() =>
                    compile_error!("generic parameters not allowed on `protocol` method");
                });
            }
            if let Some(where_clause) = &method.sig.decl.generics.where_clause {
                return TokenStream::from(quote_spanned! {
                    where_clause.span() =>
                    compile_error!("where clause not allowed on `protocol` method");
                });
            }
            if let ReturnType::Type(_, ty) = &mut method.sig.decl.output {
                let ident = Ident::new(
                    &format!("_{}_{}_rt_AssertValue", &input.ident, index),
                    Span::call_site(),
                );
                assert_stream.extend(TokenStream::from(quote_spanned! {
                    ty.span() =>
                    #[allow(non_camel_case_types)]
                    struct #ident where #ty: ::vessels::protocol::Value;
                }));
                procedure.return_type = Some(*ty.clone());
            } else {
                let m: proc_macro::TokenStream = quote! {
                    ()
                }
                .into();
                let ty = parse_macro_input!(m as Type);
                procedure.return_type = Some(ty);
            }
            let mut has_receiver = false;
            for (arg_index, argument) in method.sig.decl.inputs.iter().enumerate() {
                match argument {
                    FnArg::SelfValue(_) => {
                        return TokenStream::from(quote_spanned! {
                            argument.span() =>
                            compile_error!("cannot consume self in `protocol` method");
                        });
                    }
                    FnArg::SelfRef(self_ref) => {
                        if self_ref.mutability.is_some() {
                            procedure.mut_receiver = true;
                        }
                        has_receiver = true;
                    }
                    FnArg::Captured(argument) => {
                        let ty = &argument.ty;
                        let ident = Ident::new(
                            &format!(
                                "_{}_{}_arg_{}_AssertSerializeDeserialize",
                                &input.ident, index, arg_index
                            ),
                            Span::call_site(),
                        );
                        assert_stream.extend(TokenStream::from(quote_spanned! {
                            ty.span() =>
                            #[allow(non_camel_case_types)]
                            struct #ident where #ty: ::serde::Serialize + ::serde::de::DeserializeOwned;
                        }));
                        procedure.arg_types.push(argument.ty.clone());
                    }
                    _ => {
                        return TokenStream::from(quote_spanned! {
                            argument.span() =>
                            compile_error!("inferred or ignored argument not allowed in `protocol` method");
                        });
                    }
                };
            }
            if !has_receiver {
                return TokenStream::from(quote_spanned! {
                    method.sig.ident.span() =>
                    compile_error!("method in `protocol` has no receiver");
                });
            }
            procedure.ident = Some(method.sig.ident.clone());
        } else {
            return TokenStream::from(quote_spanned! {
                item.span() =>
                compile_error!("`protocol` expected method");
            });
        }
        procedures.push(procedure);
    }
    if procedures.is_empty() {
        return TokenStream::from(quote_spanned! {
            input.span() =>
            compile_error!("`protocol` with no methods is invalid");
        });
    }
    let ident = &input.ident;
    let protocol_shim = prefix(ident, "Protocol_Shim");
    let protocol_trait = prefix(ident, "Protocol_Trait");
    let mut m: TokenStream = quote! {
        #[doc(hidden)]
        fn into_protocol(self) -> Box<dyn #protocol_trait> where Self: Sized + 'static {
            Box::new(#protocol_shim::new(self))
        }
    }
    .into();
    input
        .items
        .push(TraitItem::Method(parse_macro_input!(m as TraitItemMethod)));
    m = quote! {
        #[doc(hidden)]
        fn IS_PROTO() where Self: Sized {}
    }
    .into();
    input
        .items
        .push(TraitItem::Method(parse_macro_input!(m as TraitItemMethod)));
    let mut ty_path = Punctuated::new();
    ty_path.push_value(PathSegment {
        arguments: PathArguments::None,
        ident: Ident::new("Send", input.ident.span()),
    });
    input
        .supertraits
        .push_value(TypeParamBound::Trait(TraitBound {
            paren_token: None,
            modifier: TraitBoundModifier::None,
            lifetimes: None,
            path: Path {
                leading_colon: None,
                segments: ty_path,
            },
        }));
    let c_remote = prefix(ident, "Concrete_Remote");
    let remote = prefix(ident, "Remote");
    let call = prefix(ident, "Call");
    let response = prefix(ident, "Response");
    let binds = generate_binds(ident, &procedures);
    let mut hasher = DefaultHasher::new();
    (ident, procedures).hash(&mut hasher);
    let hash = hasher.finish();
    let blanket_impl: TokenStream = quote! {
        impl ::vessels::protocol::Protocol for dyn #ident {
            type Call = #call;
            type Response = #response;
            type Remote = Box<dyn #remote>;
            fn remote() -> Self::Remote {
                Box::new(#c_remote::new())
            }
            const DO_NOT_IMPLEMENT_THIS_TRAIT_MANUALLY: u64 = #hash;
        }
    }
    .into();
    let mut item: TokenStream = input.into_token_stream().into();
    item.extend(blanket_impl);
    item.extend(assert_stream);
    item.extend(binds);
    item
}
