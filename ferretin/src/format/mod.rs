use crate::format_context::FormatContext;
use crate::indent::Indent;
use crate::request::Request;
use crate::styled_string::{DocumentNode, Span as StyledSpan};
use crate::verbosity::Verbosity;
use rustdoc_core::doc_ref::DocRef;
use rustdoc_types::{
    Abi, Constant, Enum, Function, FunctionPointer, GenericArg, GenericArgs, GenericBound,
    GenericParamDef, GenericParamDefKind, Generics, Id, Item, ItemEnum, Path, Span, Static, Struct,
    StructKind, Term, Trait, Type, TypeAlias, Union, VariantKind, Visibility, WherePredicate,
};
use std::{collections::HashMap, fs};

mod documentation;
mod r#enum;
mod functions;
mod impls;
mod items;
mod r#module;
mod source;
mod r#struct;
mod r#trait;
mod types;

impl Request {
    /// Format an item with automatic recursion tracking
    pub(crate) fn format_item<'a>(&'a self, item: DocRef<'a, Item>, context: &FormatContext) -> Vec<DocumentNode<'a>> {
        let mut doc_nodes = vec![];

        // Basic item information
        doc_nodes.push(DocumentNode::Span(StyledSpan::plain(format!("Item: {}\n", item.name().unwrap_or("unnamed")))));
        doc_nodes.push(DocumentNode::Span(StyledSpan::plain(format!("Kind: {:?}\n", item.kind()))));
        doc_nodes.push(DocumentNode::Span(StyledSpan::plain(format!("Visibility: {:?}\n", item.visibility))));

        if let Some(path) = item.path() {
            doc_nodes.push(DocumentNode::Span(StyledSpan::plain(format!("Defined at: {path}\n"))));
        }

        // Add documentation if available
        if let Some(docs) = self.docs_to_show(item, false, context) {
            doc_nodes.push(DocumentNode::Span(StyledSpan::plain(format!("\n{docs}\n"))));
        };

        // Handle different item types
        match item.inner() {
            ItemEnum::Module(_) => {
                doc_nodes.extend(self.format_module(item, context));
            }
            ItemEnum::Struct(struct_data) => {
                doc_nodes.extend(self.format_struct(item, item.build_ref(struct_data), context));
            }
            ItemEnum::Enum(enum_data) => {
                doc_nodes.extend(self.format_enum(item, item.build_ref(enum_data), context));
            }
            ItemEnum::Trait(trait_data) => {
                doc_nodes.extend(self.format_trait(item, item.build_ref(trait_data), context));
            }
            ItemEnum::Function(function_data) => {
                doc_nodes.push(DocumentNode::Span(StyledSpan::plain("\n")));
                doc_nodes.extend(self.format_function(item, item.build_ref(function_data), context));
            }
            ItemEnum::TypeAlias(type_alias_data) => {
                doc_nodes.extend(self.format_type_alias(item, item.build_ref(type_alias_data), context));
            }
            ItemEnum::Union(union_data) => {
                doc_nodes.extend(self.format_union(item, item.build_ref(union_data), context));
            }
            ItemEnum::Constant { type_, const_ } => {
                doc_nodes.extend(self.format_constant(item, type_, const_, context));
            }
            ItemEnum::Static(static_data) => {
                doc_nodes.extend(self.format_static(item, static_data, context));
            }
            ItemEnum::Macro(macro_def) => {
                doc_nodes.push(DocumentNode::Span(StyledSpan::plain("Macro definition:\n\n")));
                doc_nodes.push(DocumentNode::code_block(Some("rust".to_string()), macro_def.to_string()));
            }
            _ => {
                // For any other item, just print its name and kind
                doc_nodes.push(DocumentNode::Span(StyledSpan::plain("\n")));
                doc_nodes.push(DocumentNode::Span(StyledSpan::plain(format!("{:?}", item.kind()))));
                doc_nodes.push(DocumentNode::Span(StyledSpan::plain(" ")));
                doc_nodes.push(DocumentNode::Span(StyledSpan::plain(item.name().unwrap_or("<unnamed>").to_string())));
                doc_nodes.push(DocumentNode::Span(StyledSpan::plain("\n")));
            }
        }

        // Add source code if requested
        if context.include_source()
            && let Some(span) = &item.span
        {
            doc_nodes.extend(source::format_source_code(self, span, context));
        }

        doc_nodes
    }
}
