use super::*;
use crate::styled_string::{DocumentNode, Span};

impl Request {
    /// Format a trait
    pub(super) fn format_trait<'a>(
        &'a self,
        item: DocRef<'a, Item>,
        trait_data: DocRef<'a, Trait>,
        context: &FormatContext,
    ) -> Vec<DocumentNode<'a>> {
        let trait_name = item.name.as_deref().unwrap_or("<unnamed>").to_string();

        // Build trait signature
        let mut code_spans = vec![
            Span::plain("\n"),
            Span::keyword("trait"),
            Span::plain(" "),
            Span::type_name(trait_name),
        ];

        if !trait_data.generics.params.is_empty() {
            code_spans.extend(self.format_generics(&trait_data.generics));
        }

        if !trait_data.generics.where_predicates.is_empty() {
            code_spans.extend(self.format_where_clause(&trait_data.generics.where_predicates));
        }

        code_spans.push(Span::plain(" "));
        code_spans.push(Span::punctuation("{"));
        code_spans.push(Span::plain("\n"));

        // Add trait members
        for trait_item in item.id_iter(&trait_data.items) {
            // Add documentation as a comment
            if let Some(docs) = self.docs_to_show(trait_item, false, context) {
                code_spans.push(Span::plain("    "));
                code_spans.push(Span::comment(format!("/// {}", docs)));
                code_spans.push(Span::plain("\n"));
            }

            let item_name = trait_item.name.as_deref().unwrap_or("<unnamed>");

            match &trait_item.inner {
                ItemEnum::Function(f) => {
                    self.format_trait_function(&mut code_spans, f, item_name)
                }
                ItemEnum::AssocType {
                    generics,
                    bounds,
                    type_,
                } => self.format_assoc_type(&mut code_spans, generics, bounds, type_, item_name),
                ItemEnum::AssocConst { type_, value } => {
                    self.format_assoc_const(&mut code_spans, type_, value, item_name)
                }
                _ => {
                    code_spans.push(Span::plain("    "));
                    code_spans.push(Span::comment(format!("// {}: {:?}", item_name, trait_item.inner)));
                    code_spans.push(Span::plain("\n"));
                }
            }
        }

        code_spans.push(Span::punctuation("}"));
        code_spans.push(Span::plain("\n"));

        // Convert to DocumentNodes
        code_spans
            .into_iter()
            .map(DocumentNode::Span)
            .collect()
    }

    fn format_assoc_const<'a>(
        &self,
        spans: &mut Vec<Span<'a>>,
        type_: &Type,
        value: &Option<String>,
        const_name: &str,
    ) {
        spans.push(Span::plain("    "));
        spans.push(Span::keyword("const"));
        spans.push(Span::plain(" "));
        spans.push(Span::plain(const_name.to_string()));
        spans.push(Span::punctuation(":"));
        spans.push(Span::plain(" "));
        spans.extend(self.format_type(type_));

        if let Some(default_val) = value {
            spans.push(Span::plain(" "));
            spans.push(Span::operator("="));
            spans.push(Span::plain(" "));
            spans.push(Span::inline_code(default_val.clone()));
        }

        spans.push(Span::punctuation(";"));
        spans.push(Span::plain("\n"));
    }

    fn format_assoc_type<'a>(
        &self,
        spans: &mut Vec<Span<'a>>,
        generics: &Generics,
        bounds: &[GenericBound],
        type_: &Option<Type>,
        type_name: &str,
    ) {
        spans.push(Span::plain("    "));
        spans.push(Span::keyword("type"));
        spans.push(Span::plain(" "));
        spans.push(Span::type_name(type_name.to_string()));

        if !generics.params.is_empty() {
            spans.extend(self.format_generics(generics));
        }

        if !bounds.is_empty() {
            spans.push(Span::punctuation(":"));
            spans.push(Span::plain(" "));
            spans.extend(self.format_generic_bounds(bounds));
        }

        if let Some(default_type) = type_ {
            spans.push(Span::plain(" "));
            spans.push(Span::operator("="));
            spans.push(Span::plain(" "));
            spans.extend(self.format_type(default_type));
        }

        spans.push(Span::punctuation(";"));
        spans.push(Span::plain("\n"));
    }

    fn format_trait_function<'a>(&self, spans: &mut Vec<Span<'a>>, f: &Function, method_name: &str) {
        let has_default = f.has_body;

        spans.push(Span::plain("    "));
        spans.extend(self.format_function_signature(method_name, f));

        if has_default {
            spans.push(Span::plain(" "));
            spans.push(Span::punctuation("{"));
            spans.push(Span::plain(" ... "));
            spans.push(Span::punctuation("}"));
        } else {
            spans.push(Span::punctuation(";"));
        }

        spans.push(Span::plain("\n"));
    }
}
