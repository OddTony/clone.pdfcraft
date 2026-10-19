//! Default optional-content visibility for text reading (PDF 1.7 §4.10).
//! Rendering has its own interpreter; this keeps the COS-based Office/edit reader independent.
use pdfcraft_content::Op;
use pdfcraft_cos::{Dict, Document, ObjRef, Object};
use std::{collections::HashSet, rc::Rc};

const MAX_GROUPS: usize = 65_536;
const MAX_MARKED_DEPTH: usize = 256;
const MAX_EXPRESSION_DEPTH: usize = 32;
const MAX_EXPRESSION_WORK: usize = 4096;

/// Shared default group configuration plus one bounded marked-content visibility stack.
#[derive(Clone)]
pub(crate) struct Visibility {
    // None means the configuration exceeded the budget: omit optional content rather than
    // publish content whose visibility could not be determined. Ordinary content is unaffected.
    inactive: Rc<Option<HashSet<ObjRef>>>,
    stack: Vec<bool>,
    overflow: usize,
}

impl Visibility {
    /// Read the default configuration once; form scopes share its immutable group state.
    pub(crate) fn new(doc: &Document) -> Self {
        let inactive = Self::inactive(doc);
        Self { inactive: Rc::new(inactive), stack: Vec::new(), overflow: 0 }
    }

    /// Apply BaseState, ON and OFF in order; None marks configuration-budget exhaustion.
    fn inactive(doc: &Document) -> Option<HashSet<ObjRef>> {
        let mut off = HashSet::new();
        let Some(root) = doc.trailer().get(b"Root").map(|o| doc.resolve(o)) else { return Some(off) };
        let Some(oc) = root.as_dict().and_then(|d| d.get(b"OCProperties")).map(|o| doc.resolve(o)) else { return Some(off) };
        let Some(config) = oc.as_dict().and_then(|d| d.get(b"D")).map(|o| doc.resolve(o)) else { return Some(off) };
        let Some(config) = config.as_dict() else { return Some(off) };
        let mut work = 0usize;
        for (value, disabled) in [
            (oc.as_dict().and_then(|d| d.get(b"OCGs")).filter(|_| config.name(b"BaseState") == Some(b"OFF")), true),
            (config.get(b"ON"), false),
            (config.get(b"OFF"), true),
        ] {
            let Some(array) = value.map(|o| doc.resolve(o)) else { continue };
            let Some(array) = array.as_array() else { continue };
            for value in array {
                work = work.saturating_add(1);
                if work > MAX_GROUPS {
                    return None;
                }
                if let Some(r) = value.as_ref() {
                    if disabled {
                        off.insert(r);
                    } else {
                        off.remove(&r);
                    }
                }
            }
        }
        Some(off)
    }

    /// Forms inherit the caller's visibility by being entered only at visible Do operators.
    /// Their own marked-content stack cannot pop the caller's scopes.
    pub(crate) fn fresh(&self) -> Self {
        Self { inactive: self.inactive.clone(), stack: Vec::new(), overflow: 0 }
    }

    /// Report cumulative scope visibility; excess nesting stays hidden until it unwinds.
    pub(crate) fn visible(&self) -> bool {
        self.overflow == 0 && self.stack.last().copied().unwrap_or(true)
    }

    /// Tell the text reader whether this close belongs to an untracked excess scope.
    pub(crate) fn overflowing(&self) -> bool {
        self.overflow != 0
    }

    /// Update BMC/BDC/EMC scopes, resolving named OC properties in the current resources.
    pub(crate) fn apply(&mut self, doc: &Document, resources: &Dict, op: &Op) {
        match op.op.as_slice() {
            b"BMC" | b"BDC" => {
                if self.stack.len() >= MAX_MARKED_DEPTH {
                    self.overflow = self.overflow.saturating_add(1);
                    return;
                }
                let own = if op.is("BDC") && op.name(0) == Some(b"OC") {
                    op.operands.get(1).is_none_or(|o| {
                        if let Object::Name(name) = o {
                            resources
                                .get(b"Properties")
                                .map(|p| doc.resolve(p))
                                .and_then(|p| p.as_dict().and_then(|d| d.get(name)).map(|o| self.object_visible(doc, o)))
                                .unwrap_or(true)
                        } else {
                            self.object_visible(doc, o)
                        }
                    })
                } else {
                    true
                };
                self.stack.push(self.visible() && own);
            }
            b"EMC" => {
                if self.overflow != 0 {
                    self.overflow -= 1;
                } else {
                    self.stack.pop();
                }
            }
            _ => {}
        }
    }

    /// Evaluate an XObject's own OC membership without entering or decoding its stream.
    pub(crate) fn xobject_visible(&self, doc: &Document, resources: &Dict, name: &[u8]) -> bool {
        resources
            .get(b"XObject")
            .map(|o| doc.resolve(o))
            .and_then(|xo| {
                xo.as_dict().and_then(|d| d.get(name)).map(|o| {
                    let object = doc.resolve(o);
                    object.as_dict().and_then(|d| d.get(b"OC")).is_none_or(|oc| self.object_visible(doc, oc))
                })
            })
            .unwrap_or(true)
    }

    /// Evaluate an OCG or OCMD; a valid VE overrides membership policy.
    pub(crate) fn object_visible(&self, doc: &Document, value: &Object) -> bool {
        let Some(inactive) = self.inactive.as_ref() else { return false };
        let resolved = doc.resolve(value);
        let Some(dict) = resolved.as_dict() else { return true };
        if dict.name(b"Type") != Some(b"OCMD") {
            return value.as_ref().is_none_or(|r| !inactive.contains(&r));
        }
        if let Some(expression) = dict.get(b"VE") {
            let mut work = MAX_EXPRESSION_WORK;
            if let Some(visible) = self.expression(doc, expression, 0, &mut work) {
                return visible;
            }
        }
        let Some(groups) = dict.get(b"OCGs") else { return true };
        let resolved = doc.resolve(groups);
        let groups = resolved.as_array().map(Vec::as_slice).unwrap_or_else(|| std::slice::from_ref(groups));
        if groups.len() > MAX_GROUPS {
            return false;
        }
        let mut states = groups.iter().filter_map(Object::as_ref).map(|r| !inactive.contains(&r)).peekable();
        if states.peek().is_none() {
            return true;
        }
        match dict.name(b"P") {
            Some(b"AllOn") => states.all(|v| v),
            Some(b"AnyOff") => states.any(|v| !v),
            Some(b"AllOff") => states.all(|v| !v),
            _ => states.any(|v| v),
        }
    }

    /// Evaluate boolean VE nodes within depth/work limits; invalid input requests policy fallback.
    fn expression(&self, doc: &Document, value: &Object, depth: usize, work: &mut usize) -> Option<bool> {
        if depth > MAX_EXPRESSION_DEPTH || *work == 0 {
            return None;
        }
        *work -= 1;
        let inactive = self.inactive.as_ref().as_ref()?;
        let object = doc.resolve(value);
        if object.as_dict().is_some_and(|d| d.name(b"Type") == Some(b"OCG")) {
            return Some(!inactive.contains(&value.as_ref()?));
        }
        let array = object.as_array()?;
        let (operator, operands) = array.split_first()?;
        let Object::Name(operator) = operator else { return None };
        match operator.as_slice() {
            b"Not" if operands.len() == 1 => Some(!self.expression(doc, operands.first()?, depth + 1, work)?),
            b"And" | b"Or" if !operands.is_empty() => {
                let and = operator == b"And";
                let mut result = and;
                // Validate every operand, even after the result is known, within one work budget.
                for operand in operands {
                    let v = self.expression(doc, operand, depth + 1, work)?;
                    result = if and { result && v } else { result || v };
                }
                Some(result)
            }
            _ => None,
        }
    }
}
