use quiddity_syntax::{
    MetaProperty, Property, Value, format_property_line, is_properties_block, normalise_name,
    parse_block,
};

use crate::{BlockId, Error, Graph, Result};

/// The first line of `name`, trimmed: an entity's name is one line.
fn one_line_name(name: &str) -> Result<&str> {
    let name = name.lines().next().unwrap_or("").trim();
    if name.is_empty() {
        return Err(Error::EmptyName);
    }
    Ok(name)
}

impl Graph {
    /// Creates an entity: a root block named `name`, with an initial
    /// properties block if `properties` isn't empty.
    pub fn create_entity(
        &self,
        name: &str,
        properties: &[(&str, Option<Value>)],
    ) -> Result<BlockId> {
        let entity = self.create_block(None, None, name)?;
        if !properties.is_empty() {
            let lines: Vec<_> = properties
                .iter()
                .map(|(key, value)| format_property_line(key, value.as_ref()))
                .collect();
            self.create_block(Some(entity), None, &lines.join("\n"))?;
        }
        Ok(entity)
    }

    /// An entity's name: the first line of its root block.
    pub fn entity_name(&self, entity: BlockId) -> Result<String> {
        Ok(self
            .text(entity)?
            .lines()
            .next()
            .unwrap_or("")
            .trim()
            .to_owned())
    }

    /// Renames an entity, keeping the old name as an `alias` so that links
    /// written against it (including by a device that was offline) still
    /// resolve. Returns false, and changes nothing, if the name is the same.
    ///
    /// Only the first line of `name` is used, as an entity's name is one line.
    /// A name that differs only in case or spacing is a rename but needs no
    /// alias, since names are matched that way anyway.
    pub fn rename_entity(&self, entity: BlockId, name: &str) -> Result<bool> {
        let name = one_line_name(name)?;
        let old = self.entity_name(entity)?;
        if name == old {
            return Ok(false);
        }
        let aliases = self.aliases(entity)?;
        self.set_text(entity, name)?;
        let key = normalise_name(&old);
        if !old.is_empty() && key != normalise_name(name) && !aliases.contains(&key) {
            let line = format_property_line("alias", Some(&Value::Text(old)));
            match self.properties_block(entity)? {
                Some(block) => {
                    let end = self.text_len_utf16(block)?;
                    self.splice_text(block, end, 0, &format!("\n{line}"))?;
                }
                None => {
                    self.create_block(Some(entity), Some(0), &line)?;
                }
            }
        }
        Ok(true)
    }

    /// Corrects an entity's name without keeping the old one as an alias, for
    /// a name nothing has relied on yet: a mistyped link whose entity was
    /// made from it a moment ago, say. Anything that may have linked to the
    /// old name will no longer find the entity, so use [`Graph::rename_entity`]
    /// unless that is known not to matter. Returns false, and changes nothing,
    /// if the name is the same.
    pub fn correct_entity_name(&self, entity: BlockId, name: &str) -> Result<bool> {
        let name = one_line_name(name)?;
        if name == self.entity_name(entity)? {
            return Ok(false);
        }
        self.set_text(entity, name)?;
        Ok(true)
    }

    /// The entity's aliases, as names are matched.
    fn aliases(&self, entity: BlockId) -> Result<Vec<String>> {
        fn names(value: &Value, out: &mut Vec<String>) {
            match value {
                Value::Text(s) => out.push(normalise_name(s)),
                Value::Link(link) => out.push(normalise_name(&link.name)),
                Value::List(items) => items.iter().for_each(|v| names(v, out)),
                Value::BlockRef(_) => {}
            }
        }
        let mut out = Vec::new();
        for p in self.entity_properties(entity)? {
            if normalise_name(&p.key) == "alias" {
                p.value.iter().for_each(|v| names(v, &mut out));
            }
        }
        Ok(out)
    }

    /// The entity's initial properties block, if its first child is one.
    pub fn properties_block(&self, entity: BlockId) -> Result<Option<BlockId>> {
        match self.children(entity).first() {
            Some(&first) if is_properties_block(&self.text(first)?) => Ok(Some(first)),
            _ => Ok(None),
        }
    }

    /// The entity's own properties, from its initial properties block.
    pub fn entity_properties(&self, entity: BlockId) -> Result<Vec<Property>> {
        match self.properties_block(entity)? {
            Some(block) => self.block_properties(block),
            None => Ok(Vec::new()),
        }
    }

    /// The meta-properties in the entity's properties block.
    pub fn entity_meta(&self, entity: BlockId) -> Result<Vec<MetaProperty>> {
        match self.properties_block(entity)? {
            Some(block) => Ok(parse_block(&self.text(block)?).meta),
            None => Ok(Vec::new()),
        }
    }

    /// The properties written in a block's text.
    pub fn block_properties(&self, block: BlockId) -> Result<Vec<Property>> {
        Ok(parse_block(&self.text(block)?).properties)
    }

    /// Returns the block's `id` property, adding a new one if it has none, so
    /// that it can be referred to as `((id))`.
    pub fn ensure_ref_id(&self, block: BlockId) -> Result<String> {
        let text = self.text(block)?;
        let existing = parse_block(&text)
            .properties
            .into_iter()
            .find_map(|p| match p.value {
                Some(Value::Text(id)) if p.key.eq_ignore_ascii_case("id") => Some(id),
                _ => None,
            });
        if let Some(id) = existing {
            return Ok(id);
        }
        let id = uuid::Uuid::now_v7().to_string();
        let line = format_property_line("id", Some(&Value::Text(id.clone())));
        let sep = if text.is_empty() || text.ends_with('\n') {
            ""
        } else {
            "\n"
        };
        self.set_text(block, &format!("{text}{sep}{line}"))?;
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> Option<Value> {
        Some(Value::Text(s.to_owned()))
    }

    #[test]
    fn entity_properties() -> Result<()> {
        let graph = Graph::new();
        let jane = graph.create_entity(
            "Jane",
            &[("type", text("person")), ("born", text("1970-01-01"))],
        )?;
        graph.create_block(Some(jane), None, "Met at #event(summit)")?;
        assert_eq!(graph.entity_name(jane)?, "Jane");
        let props = graph.entity_properties(jane)?;
        assert_eq!(
            props.iter().map(|p| p.key.as_str()).collect::<Vec<_>>(),
            ["type", "born"]
        );

        let plain = graph.create_entity("Plain", &[])?;
        graph.create_block(Some(plain), None, "not:: only\nproperties")?;
        assert_eq!(graph.properties_block(plain)?, None);
        Ok(())
    }

    fn aliases_of(graph: &Graph, entity: BlockId) -> Vec<String> {
        graph
            .entity_properties(entity)
            .unwrap()
            .into_iter()
            .filter(|p| p.key == "alias")
            .filter_map(|p| match p.value {
                Some(Value::Text(s)) => Some(s),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn renaming_keeps_the_old_name_as_an_alias() -> Result<()> {
        let graph = Graph::new();
        let jane = graph.create_entity("Jane", &[("type", text("person"))])?;
        assert!(graph.rename_entity(jane, "Jane Doe")?);
        assert_eq!(graph.entity_name(jane)?, "Jane Doe");
        assert_eq!(aliases_of(&graph, jane), ["Jane"]);
        assert_eq!(
            graph.text(graph.properties_block(jane)?.unwrap())?,
            "type:: person\nalias:: Jane",
            "added to the properties block"
        );
        // A second rename adds another, and the first stays.
        assert!(graph.rename_entity(jane, "J. Doe")?);
        assert_eq!(aliases_of(&graph, jane), ["Jane", "Jane Doe"]);
        Ok(())
    }

    #[test]
    fn an_entity_with_no_properties_block_gets_one_first() -> Result<()> {
        let graph = Graph::new();
        let page = graph.create_entity("Old", &[])?;
        let note = graph.create_block(Some(page), None, "a note")?;
        graph.rename_entity(page, "New")?;
        let first = graph.children(page)[0];
        assert_eq!(graph.text(first)?, "alias:: Old");
        assert_eq!(graph.properties_block(page)?, Some(first));
        assert_eq!(graph.children(page)[1], note, "ahead of the note");
        Ok(())
    }

    #[test]
    fn the_same_name_changes_nothing_and_an_empty_one_is_refused() -> Result<()> {
        let graph = Graph::new();
        let page = graph.create_entity("Page", &[])?;
        let before = graph.version();
        assert!(!graph.rename_entity(page, "  Page \n second line")?);
        assert_eq!(graph.version(), before);
        for empty in ["", "   ", "\nsecond"] {
            assert!(matches!(
                graph.rename_entity(page, empty),
                Err(Error::EmptyName)
            ));
        }
        assert_eq!(graph.entity_name(page)?, "Page");
        Ok(())
    }

    #[test]
    fn only_the_first_line_of_a_name_is_used() -> Result<()> {
        let graph = Graph::new();
        let page = graph.create_entity("Old", &[])?;
        graph.rename_entity(page, "  New name  \nignored")?;
        assert_eq!(graph.text(page)?, "New name");
        Ok(())
    }

    #[test]
    fn a_change_of_case_or_spacing_is_a_rename_without_an_alias() -> Result<()> {
        let graph = Graph::new();
        let page = graph.create_entity("my page", &[])?;
        assert!(graph.rename_entity(page, "My Page")?);
        assert_eq!(graph.entity_name(page)?, "My Page");
        assert_eq!(graph.properties_block(page)?, None, "nothing to alias");
        Ok(())
    }

    #[test]
    fn correcting_a_name_leaves_no_alias() -> Result<()> {
        let graph = Graph::new();
        let page = graph.create_entity("Fo", &[])?;
        assert!(graph.correct_entity_name(page, " Foo \nignored")?);
        assert_eq!(graph.entity_name(page)?, "Foo");
        assert_eq!(graph.properties_block(page)?, None);
        assert!(!graph.correct_entity_name(page, "Foo")?, "same name");
        assert!(matches!(
            graph.correct_entity_name(page, "  "),
            Err(Error::EmptyName)
        ));
        assert_eq!(graph.entity_name(page)?, "Foo");
        Ok(())
    }

    #[test]
    fn a_name_already_an_alias_is_not_added_again() -> Result<()> {
        let graph = Graph::new();
        let page = graph.create_entity("Alpha", &[("alias", text("Beta"))])?;
        graph.rename_entity(page, "Beta")?;
        graph.rename_entity(page, "Alpha")?;
        // Alpha was added when it stopped being the name; Beta was there.
        assert_eq!(aliases_of(&graph, page), ["Beta", "Alpha"]);

        // Aliases written as a list count, and are matched as names are.
        let listed = Value::List(vec![Value::Text("X".into()), Value::Text("Y".into())]);
        let page = graph.create_entity("P", &[("alias", Some(listed))])?;
        graph.rename_entity(page, "y")?;
        graph.rename_entity(page, "Z")?;
        let alias_lines = |p| -> usize {
            let props = graph.entity_properties(p).unwrap();
            props.iter().filter(|p| p.key == "alias").count()
        };
        assert_eq!(alias_lines(page), 2, "the list, and P; `y` was already Y");
        Ok(())
    }

    #[test]
    fn awkward_old_names_survive_as_aliases() -> Result<()> {
        let graph = Graph::new();
        let page = graph.create_entity("a, b [c] #d", &[])?;
        graph.rename_entity(page, "Plain")?;
        assert_eq!(aliases_of(&graph, page), ["a, b [c] #d"]);
        Ok(())
    }

    #[test]
    fn ref_ids_are_stable() -> Result<()> {
        let graph = Graph::new();
        let page = graph.create_entity("Page", &[])?;
        let block = graph.create_block(Some(page), None, "Quotable")?;
        let id = graph.ensure_ref_id(block)?;
        assert_eq!(graph.ensure_ref_id(block)?, id);
        assert_eq!(graph.text(block)?, format!("Quotable\nid:: {id}"));

        let other = graph.create_block(Some(page), None, "status:: done")?;
        let id = graph.ensure_ref_id(other)?;
        assert_ne!(id, "done", "only an `id` property is an ID");
        assert_eq!(graph.text(other)?, format!("status:: done\nid:: {id}"));

        let empty = graph.create_block(Some(page), None, "")?;
        let id = graph.ensure_ref_id(empty)?;
        assert_eq!(graph.text(empty)?, format!("id:: {id}"));
        Ok(())
    }
}
