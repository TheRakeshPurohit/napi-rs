use crate::{bindgen_runtime::TypeName, ValueType};

use super::Value;

#[derive(Clone, Copy)]
pub struct JsUndefined(pub(crate) Value);

impl TypeName for JsUndefined {
  fn type_name() -> &'static str {
    "undefined"
  }

  fn value_type() -> crate::ValueType {
    ValueType::Undefined
  }
}
