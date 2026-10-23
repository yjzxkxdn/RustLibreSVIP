//! Port of `libresvip.plugins.svip.msnrbf.svip_writer` (which is itself ported
//! from QNrbf by SineStriker).
//!
//! The writer is very sensitive to object-id allocation and record ordering, so
//! the Python algorithm is reproduced statement by statement:
//!
//! * `enq` hands out ids to objects that are written later (FIFO),
//! * strings and libraries grab an id without being queued,
//! * `SerialItemList`1+_buf` reuses the id of `_buf`,
//! * a dataclass whose type has a `value` attribute (`NoteHeadTag`,
//!   `ReverbPreset`) is inlined with a *negative* object id,
//! * class arrays are padded to the next power of two (minimum 4) and the
//!   padding is emitted as `ObjectNullMultiple*` records.

use std::collections::{HashMap, VecDeque};

use libresvip_core::error::Result;

use super::super::xstudio::{
    xs_error, DeclType, XsClass, XsField, XsValue, LIBRARY_NAME_SINGING_TOOL_MODEL,
};
use super::{
    write_length_prefixed_string, write_pascal_string, write_primitive, write_u8, ArrayInfo,
    BinaryArrayType, BinaryType, BinaryTypeInfo, ClassInfo, ClassTypeInfo, MemberTypeInfo,
    PrimValue, PrimitiveType, RECORD_ARRAY_SINGLE_PRIMITIVE, RECORD_BINARY_ARRAY,
    RECORD_BINARY_LIBRARY, RECORD_BINARY_OBJECT_STRING, RECORD_CLASS_WITH_ID,
    RECORD_CLASS_WITH_MEMBERS, RECORD_CLASS_WITH_MEMBERS_AND_TYPES, RECORD_MEMBER_PRIMITIVE_TYPED,
    RECORD_MEMBER_REFERENCE, RECORD_MESSAGE_END, RECORD_OBJECT_NULL,
    RECORD_OBJECT_NULL_MULTIPLE, RECORD_OBJECT_NULL_MULTIPLE_256,
    RECORD_SERIALIZED_STREAM_HEADER, RECORD_SYSTEM_CLASS_WITH_MEMBERS_AND_TYPES,
};

/// A member value in an outgoing record.
#[derive(Clone, Debug)]
pub enum OutValue {
    Primitive(PrimitiveType, PrimValue),
    Record(Box<OutRecord>),
}

/// An element of an outgoing `BinaryArray`.
#[derive(Clone, Debug)]
pub enum OutArrayElement {
    Primitive(PrimitiveType, PrimValue),
    String(String),
    Record(OutRecord),
}

/// An outgoing class record.
#[derive(Clone, Debug)]
pub struct OutClass {
    pub record_type: u8,
    pub class_info: ClassInfo,
    pub member_types: MemberTypeInfo,
    pub library_id: Option<i32>,
    pub values: Vec<OutValue>,
}

/// An outgoing `ClassWithId` record.
#[derive(Clone, Debug)]
pub struct OutClassWithId {
    pub object_id: i32,
    pub metadata_id: i32,
    pub member_types: MemberTypeInfo,
    pub values: Vec<OutValue>,
}

/// An outgoing `BinaryArray` record.
#[derive(Clone, Debug)]
pub struct OutBinaryArray {
    pub object_id: i32,
    pub array_type: BinaryArrayType,
    pub rank: i32,
    pub lengths: Vec<i32>,
    pub lower_bounds: Option<Vec<i32>>,
    pub binary_type: BinaryType,
    pub info: BinaryTypeInfo,
    pub elements: Option<Vec<OutArrayElement>>,
}

/// An outgoing NRBF record.
#[derive(Clone, Debug)]
pub enum OutRecord {
    Header {
        root_id: i32,
        header_id: i32,
        major_version: i32,
        minor_version: i32,
    },
    Class(OutClass),
    ClassWithId(OutClassWithId),
    BinaryObjectString {
        object_id: i32,
        value: String,
    },
    BinaryArray(OutBinaryArray),
    MemberPrimitiveTyped {
        primitive_type: PrimitiveType,
        value: PrimValue,
    },
    MemberReference {
        id_ref: i32,
    },
    ObjectNull,
    ObjectNullMultiple256 {
        null_count: u8,
    },
    ObjectNullMultiple {
        null_count: i32,
    },
    BinaryLibrary {
        library_id: i32,
        library_name: String,
    },
    ArraySinglePrimitive {
        array_info: ArrayInfo,
        primitive_type: PrimitiveType,
        values: Vec<PrimValue>,
    },
    MessageEnd,
}

#[derive(Clone, Debug)]
struct ClassDefOut {
    object_id: i32,
    member_types: MemberTypeInfo,
}

#[derive(Clone, Debug)]
enum Reference {
    /// A model object that is written when its id is popped from the queue.
    Node {
        node: XsClass,
        subcon_class_name: Option<String>,
    },
    /// An already built record (`BinaryArray`, `ArraySinglePrimitive`).
    Record(OutRecord),
}

struct SvipWriter {
    id_max: i32,
    queue: VecDeque<i32>,
    model_library_id: i32,
    lib_library_id: i32,
    class_defs: HashMap<String, ClassDefOut>,
    references: HashMap<i32, Reference>,
    records: Vec<OutRecord>,
    file_version: String,
}

/// `SvipWriter.write` — serialise `model` as a `.svip` file body.
pub fn write(version: &str, model: &XsClass) -> Result<Vec<u8>> {
    let mut writer = SvipWriter {
        id_max: 0,
        queue: VecDeque::new(),
        model_library_id: 0,
        lib_library_id: 0,
        class_defs: HashMap::new(),
        references: HashMap::new(),
        records: Vec::new(),
        file_version: String::new(),
    };
    writer.write(version, model)
}

impl SvipWriter {
    fn write(&mut self, version: &str, model: &XsClass) -> Result<Vec<u8>> {
        let characters: Vec<char> = version.chars().collect();
        let (magic, file_version): (String, String) = if characters.len() >= 4 {
            (
                characters[..4].iter().collect(),
                characters[4..].iter().collect(),
            )
        } else {
            (version.to_owned(), String::new())
        };
        self.file_version = file_version.clone();

        self.id_max = 1;
        let app_model_id = self.id_max;
        self.records.push(OutRecord::Header {
            root_id: app_model_id,
            header_id: -1,
            major_version: 1,
            minor_version: 0,
        });
        self.model_library_id = self.write_library(LIBRARY_NAME_SINGING_TOOL_MODEL);
        self.lib_library_id = self.write_library(super::super::xstudio::LIBRARY_NAME_SINGING_TOOL_LIBRARY);
        let root = self.write_dataclass(model, app_model_id, None)?;
        self.records.push(root);
        while let Some(id) = self.queue.pop_front() {
            match self.references.remove(&id) {
                Some(Reference::Node {
                    node,
                    subcon_class_name,
                }) => {
                    let record = self.write_dataclass(&node, id, subcon_class_name.as_deref())?;
                    self.records.push(record);
                }
                Some(Reference::Record(record)) => self.records.push(record),
                None => {
                    return Err(xs_error(format!(
                        "object {id} was queued but never registered"
                    )))
                }
            }
        }
        self.records.push(OutRecord::MessageEnd);

        let mut out = Vec::new();
        write_pascal_string(&mut out, &magic)?;
        write_pascal_string(&mut out, &file_version)?;
        for record in &self.records {
            serialize_record(&mut out, record)?;
        }
        Ok(out)
    }

    fn write_library(&mut self, library_name: &str) -> i32 {
        self.id_max += 1;
        let library_id = self.id_max;
        self.records.push(OutRecord::BinaryLibrary {
            library_id,
            library_name: library_name.to_owned(),
        });
        library_id
    }

    fn enq(&mut self) -> i32 {
        self.id_max += 1;
        self.queue.push_back(self.id_max);
        self.id_max
    }

    fn allocate_string_id(&mut self) -> i32 {
        self.id_max += 1;
        self.id_max
    }

    fn register_reference(&mut self, object_id: i32, node: &XsClass, subcon_class_name: &str) {
        if self.references.contains_key(&object_id) {
            return;
        }
        let stored = strip_generic_argument(subcon_class_name);
        self.references.insert(
            object_id,
            Reference::Node {
                node: node.clone(),
                subcon_class_name: stored,
            },
        );
    }

    fn write_dataclass(
        &mut self,
        node: &XsClass,
        object_id: i32,
        subcon_class_name: Option<&str>,
    ) -> Result<OutRecord> {
        let mut class_name = node.name.to_owned();
        if let Some(subcon) = subcon_class_name {
            if class_name.ends_with("List") && subcon != class_name {
                class_name = format!(
                    "{class_name}`1[[{subcon}, {LIBRARY_NAME_SINGING_TOOL_MODEL}]]"
                );
            }
        }
        if let Some(definition) = self.class_defs.get(&class_name) {
            let metadata_id = definition.object_id;
            let member_types = definition.member_types.clone();
            let values = self.write_member_values(node)?;
            return Ok(OutRecord::ClassWithId(OutClassWithId {
                object_id,
                metadata_id,
                member_types,
                values,
            }));
        }

        let record_type = if class_name.starts_with("System.") {
            RECORD_SYSTEM_CLASS_WITH_MEMBERS_AND_TYPES
        } else {
            RECORD_CLASS_WITH_MEMBERS_AND_TYPES
        };
        let library_id = if record_type == RECORD_CLASS_WITH_MEMBERS_AND_TYPES {
            if class_name.starts_with("SingingTool.Model.") {
                Some(self.model_library_id)
            } else if class_name.starts_with("SingingTool.Library.") {
                Some(self.lib_library_id)
            } else {
                return Err(xs_error(format!(
                    "cannot determine the .NET library of {class_name}"
                )));
            }
        } else {
            None
        };
        let mut member_names = Vec::with_capacity(node.fields.len());
        let mut member_types = MemberTypeInfo {
            binary_types: Vec::with_capacity(node.fields.len()),
            additional_infos: Vec::with_capacity(node.fields.len()),
        };
        for field in &node.fields {
            if field.alias == "_editedPowerLine" && self.file_version != "7.0.0" {
                continue;
            }
            let (binary_type, info) = type_info_of(field, self.model_library_id, self.lib_library_id)?;
            member_names.push(field.alias.to_owned());
            member_types.binary_types.push(binary_type);
            member_types.additional_infos.push(info);
        }
        let values = self.write_member_values(node)?;
        self.class_defs.insert(
            class_name.clone(),
            ClassDefOut {
                object_id,
                member_types: member_types.clone(),
            },
        );
        Ok(OutRecord::Class(OutClass {
            record_type,
            class_info: ClassInfo {
                object_id,
                name: class_name,
                member_names,
            },
            member_types,
            library_id,
            values,
        }))
    }

    fn write_member_values(&mut self, node: &XsClass) -> Result<Vec<OutValue>> {
        let mut values = Vec::with_capacity(node.fields.len());
        for field in &node.fields {
            if field.alias == "_editedPowerLine" && self.file_version != "7.0.0" {
                continue;
            }
            values.push(self.write_member_value(field)?);
        }
        Ok(values)
    }

    fn write_member_value(&mut self, field: &XsField) -> Result<OutValue> {
        match &field.value {
            XsValue::Null => Ok(OutValue::Record(Box::new(OutRecord::ObjectNull))),
            XsValue::Str(value) => {
                let object_id = self.allocate_string_id();
                Ok(OutValue::Record(Box::new(OutRecord::BinaryObjectString {
                    object_id,
                    value: value.clone(),
                })))
            }
            XsValue::Bool(value) => Ok(OutValue::Primitive(
                PrimitiveType::Boolean,
                PrimValue::Boolean(*value),
            )),
            XsValue::Int(value) => {
                let value = i32::try_from(*value)
                    .map_err(|_| xs_error(format!("{} does not fit into Int32", field.alias)))?;
                Ok(OutValue::Primitive(
                    PrimitiveType::Int32,
                    PrimValue::Int32(value),
                ))
            }
            XsValue::F32(value) => Ok(OutValue::Primitive(
                PrimitiveType::Single,
                PrimValue::Single(*value),
            )),
            XsValue::F64(value) => Ok(OutValue::Primitive(
                PrimitiveType::Double,
                PrimValue::Double(*value),
            )),
            XsValue::Bytes(bytes) => Ok(OutValue::Record(Box::new(
                self.create_primitive_array(bytes)?,
            ))),
            XsValue::List(items) => {
                let DeclType::List { declared } = field.decl else {
                    return Err(xs_error(format!("{} is not declared as a list", field.alias)));
                };
                let type_name = match items.first() {
                    Some(XsValue::Class(node)) => node.name,
                    Some(other) => {
                        return Err(xs_error(format!(
                            "{} contains a non-class array element ({})",
                            field.alias,
                            other.kind()
                        )))
                    }
                    None => declared,
                };
                let reference = self.write_binary_array(items, type_name)?;
                Ok(OutValue::Record(Box::new(reference)))
            }
            XsValue::Class(node) => match field.decl {
                DeclType::Class { name, generic } => {
                    let type_name = class_type_name(name, generic);
                    let object_id = self.enq();
                    self.register_reference(object_id, node, &type_name);
                    Ok(OutValue::Record(Box::new(OutRecord::MemberReference {
                        id_ref: object_id,
                    })))
                }
                DeclType::SharedClass { name, generic } => {
                    let type_name = class_type_name(name, generic);
                    let object_id = self.id_max;
                    self.register_reference(object_id, node, &type_name);
                    Ok(OutValue::Record(Box::new(OutRecord::MemberReference {
                        id_ref: object_id,
                    })))
                }
                DeclType::InlineClass { name } => {
                    self.id_max += 1;
                    let object_id = self.id_max;
                    let record = self.write_dataclass(node, -object_id, Some(name))?;
                    Ok(OutValue::Record(Box::new(record)))
                }
                _ => Err(xs_error(format!(
                    "{} holds a class value but is not declared as one",
                    field.alias
                ))),
            },
        }
    }

    fn write_null_array(&self, value: usize) -> OutRecord {
        match value {
            1 => OutRecord::ObjectNull,
            value if value <= 256 => OutRecord::ObjectNullMultiple256 {
                null_count: value as u8,
            },
            value => OutRecord::ObjectNullMultiple {
                null_count: i32::try_from(value).unwrap_or(i32::MAX),
            },
        }
    }

    fn write_binary_array(&mut self, values: &[XsValue], type_name: &str) -> Result<OutRecord> {
        let object_id = self.enq();
        let padded_length = if values.is_empty() {
            4
        } else {
            std::cmp::max(4, values.len().next_power_of_two())
        };
        let array_type_name = type_name
            .replace("InstrumentTrack", "ITrack")
            .replace("SingingTrack", "ITrack");
        let mut elements = Vec::with_capacity(values.len() + 1);
        for value in values {
            let node = value.as_class()?;
            let element_id = self.enq();
            self.register_reference(element_id, node, type_name);
            elements.push(OutArrayElement::Record(OutRecord::MemberReference {
                id_ref: element_id,
            }));
        }
        if values.len() < padded_length {
            elements.push(OutArrayElement::Record(
                self.write_null_array(padded_length - values.len()),
            ));
        }
        let record = OutRecord::BinaryArray(OutBinaryArray {
            object_id,
            array_type: BinaryArrayType::Single,
            rank: 1,
            lengths: vec![i32::try_from(padded_length)
                .map_err(|_| xs_error("array is too long"))?],
            lower_bounds: None,
            binary_type: BinaryType::Class,
            info: BinaryTypeInfo::Class(ClassTypeInfo {
                type_name: array_type_name,
                library_id: self.model_library_id,
            }),
            elements: Some(elements),
        });
        self.references
            .insert(object_id, Reference::Record(record.clone()));
        Ok(OutRecord::MemberReference {
            id_ref: object_id,
        })
    }

    fn create_primitive_array(&mut self, values: &[u8]) -> Result<OutRecord> {
        let object_id = self.enq();
        let record = OutRecord::ArraySinglePrimitive {
            array_info: ArrayInfo {
                object_id,
                length: i32::try_from(values.len())
                    .map_err(|_| xs_error("byte array is too long"))?,
            },
            primitive_type: PrimitiveType::Byte,
            values: values.iter().map(|value| PrimValue::Byte(*value)).collect(),
        };
        self.references
            .insert(object_id, Reference::Record(record.clone()));
        Ok(OutRecord::MemberReference {
            id_ref: object_id,
        })
    }
}

/// `create_reference` strips a generic class name down to its type argument
/// (``A`1[[B, …]]`` → `B`).
fn strip_generic_argument(class_name: &str) -> Option<String> {
    if !class_name.contains("`1") {
        return Some(class_name.to_owned());
    }
    let after_open = match class_name.split_once("[[") {
        Some((_, rest)) => rest,
        None => class_name,
    };
    let head = match after_open.split_once(", ") {
        Some((head, _)) => head,
        None => after_open,
    };
    Some(head.to_owned())
}

/// The member type name of a dataclass field: a `…List` class name is wrapped
/// with its generic argument, exactly like the Python writer.
fn class_type_name(name: &str, generic: Option<&str>) -> String {
    match generic {
        Some(generic) if name.ends_with("List") && generic != name => {
            format!("{name}`1[[{generic}, {LIBRARY_NAME_SINGING_TOOL_MODEL}]]")
        }
        _ => name.to_owned(),
    }
}

fn type_info_of(
    field: &XsField,
    model_library_id: i32,
    lib_library_id: i32,
) -> Result<(BinaryType, BinaryTypeInfo)> {
    Ok(match field.decl {
        DeclType::Str => (BinaryType::String, BinaryTypeInfo::None),
        DeclType::Bool => (
            BinaryType::Primitive,
            BinaryTypeInfo::Primitive(PrimitiveType::Boolean),
        ),
        DeclType::Int => (
            BinaryType::Primitive,
            BinaryTypeInfo::Primitive(PrimitiveType::Int32),
        ),
        DeclType::F32 => (
            BinaryType::Primitive,
            BinaryTypeInfo::Primitive(PrimitiveType::Single),
        ),
        DeclType::F64 => (
            BinaryType::Primitive,
            BinaryTypeInfo::Primitive(PrimitiveType::Double),
        ),
        DeclType::Bytes => (
            BinaryType::PrimitiveArray,
            BinaryTypeInfo::PrimitiveArray(PrimitiveType::Byte),
        ),
        DeclType::List { declared } => (
            BinaryType::Class,
            BinaryTypeInfo::Class(ClassTypeInfo {
                type_name: format!("{declared}[]"),
                library_id: model_library_id,
            }),
        ),
        DeclType::Class { name, generic } => {
            class_type_info(class_type_name(name, generic), model_library_id, lib_library_id)
        }
        DeclType::SharedClass { name, generic } => {
            class_type_info(class_type_name(name, generic), model_library_id, lib_library_id)
        }
        DeclType::InlineClass { name } => {
            class_type_info(class_type_name(name, None), model_library_id, lib_library_id)
        }
        DeclType::Unknown => {
            return Err(xs_error(format!(
                "{} has no declared .NET type",
                field.alias
            )))
        }
    })
}

fn class_type_info(
    type_name: String,
    model_library_id: i32,
    lib_library_id: i32,
) -> (BinaryType, BinaryTypeInfo) {
    if type_name.starts_with("System.") {
        (BinaryType::SystemClass, BinaryTypeInfo::SystemClass(type_name))
    } else {
        let library_id = if type_name.starts_with("SingingTool.Model.") {
            model_library_id
        } else {
            lib_library_id
        };
        (
            BinaryType::Class,
            BinaryTypeInfo::Class(ClassTypeInfo {
                type_name,
                library_id,
            }),
        )
    }
}

// ---------------------------------------------------------------------------
// Serialisation
// ---------------------------------------------------------------------------

fn serialize_class_info(out: &mut Vec<u8>, class_info: &ClassInfo) -> Result<()> {
    super::write_i32(out, class_info.object_id);
    write_length_prefixed_string(out, &class_info.name);
    super::write_i32(
        out,
        i32::try_from(class_info.member_names.len())
            .map_err(|_| xs_error("too many class members"))?,
    );
    for name in &class_info.member_names {
        write_length_prefixed_string(out, name);
    }
    Ok(())
}

fn serialize_member_types(out: &mut Vec<u8>, member_types: &MemberTypeInfo) -> Result<()> {
    if member_types.binary_types.len() != member_types.additional_infos.len() {
        return Err(xs_error("member type information is inconsistent"));
    }
    for binary_type in &member_types.binary_types {
        write_u8(out, binary_type.to_byte());
    }
    for info in &member_types.additional_infos {
        serialize_binary_type_info(out, info)?;
    }
    Ok(())
}

fn serialize_binary_type_info(out: &mut Vec<u8>, info: &BinaryTypeInfo) -> Result<()> {
    match info {
        BinaryTypeInfo::None => {}
        BinaryTypeInfo::Primitive(primitive_type) => {
            write_u8(out, primitive_type.to_byte());
        }
        BinaryTypeInfo::SystemClass(name) => write_length_prefixed_string(out, name),
        BinaryTypeInfo::Class(class_type) => {
            write_length_prefixed_string(out, &class_type.type_name);
            super::write_i32(out, class_type.library_id);
        }
        BinaryTypeInfo::PrimitiveArray(primitive_type) => {
            write_u8(out, primitive_type.to_byte());
        }
    }
    Ok(())
}

fn serialize_values(
    out: &mut Vec<u8>,
    values: &[OutValue],
    member_types: &MemberTypeInfo,
) -> Result<()> {
    if values.len() != member_types.binary_types.len() {
        return Err(xs_error(format!(
            "record has {} values but {} member types",
            values.len(),
            member_types.binary_types.len()
        )));
    }
    for (value, binary_type) in values.iter().zip(member_types.binary_types.iter()) {
        match value {
            OutValue::Primitive(primitive_type, primitive) => {
                if *binary_type != BinaryType::Primitive {
                    return Err(xs_error("primitive value in a non-primitive member"));
                }
                let _ = primitive_type;
                write_primitive(out, primitive)?;
            }
            OutValue::Record(record) => {
                if *binary_type == BinaryType::Primitive {
                    return Err(xs_error("record value in a primitive member"));
                }
                serialize_record(out, record)?;
            }
        }
    }
    Ok(())
}

fn serialize_record(out: &mut Vec<u8>, record: &OutRecord) -> Result<()> {
    match record {
        OutRecord::Header {
            root_id,
            header_id,
            major_version,
            minor_version,
        } => {
            write_u8(out, RECORD_SERIALIZED_STREAM_HEADER);
            super::write_i32(out, *root_id);
            super::write_i32(out, *header_id);
            super::write_i32(out, *major_version);
            super::write_i32(out, *minor_version);
        }
        OutRecord::Class(class) => {
            write_u8(out, class.record_type);
            serialize_class_info(out, &class.class_info)?;
            if matches!(
                class.record_type,
                RECORD_SYSTEM_CLASS_WITH_MEMBERS_AND_TYPES | RECORD_CLASS_WITH_MEMBERS_AND_TYPES
            ) {
                serialize_member_types(out, &class.member_types)?;
            }
            if matches!(
                class.record_type,
                RECORD_CLASS_WITH_MEMBERS | RECORD_CLASS_WITH_MEMBERS_AND_TYPES
            ) {
                let library_id = class
                    .library_id
                    .ok_or_else(|| xs_error("class record without a library id"))?;
                super::write_i32(out, library_id);
            }
            serialize_values(out, &class.values, &class.member_types)?;
        }
        OutRecord::ClassWithId(class) => {
            write_u8(out, RECORD_CLASS_WITH_ID);
            super::write_i32(out, class.object_id);
            super::write_i32(out, class.metadata_id);
            serialize_values(out, &class.values, &class.member_types)?;
        }
        OutRecord::BinaryObjectString { object_id, value } => {
            write_u8(out, RECORD_BINARY_OBJECT_STRING);
            super::write_i32(out, *object_id);
            write_length_prefixed_string(out, value);
        }
        OutRecord::BinaryArray(array) => {
            write_u8(out, RECORD_BINARY_ARRAY);
            super::write_i32(out, array.object_id);
            write_u8(out, array.array_type.to_byte());
            super::write_i32(out, array.rank);
            for length in &array.lengths {
                super::write_i32(out, *length);
            }
            if array.array_type.has_lower_bounds() {
                let bounds = array
                    .lower_bounds
                    .as_ref()
                    .ok_or_else(|| xs_error("array with offset bounds is missing them"))?;
                for bound in bounds {
                    super::write_i32(out, *bound);
                }
            }
            write_u8(out, array.binary_type.to_byte());
            serialize_binary_type_info(out, &array.info)?;
            if let Some(elements) = &array.elements {
                for element in elements {
                    match element {
                        OutArrayElement::Primitive(_, value) => write_primitive(out, value)?,
                        OutArrayElement::String(value) => {
                            write_length_prefixed_string(out, value)
                        }
                        OutArrayElement::Record(record) => serialize_record(out, record)?,
                    }
                }
            }
        }
        OutRecord::MemberPrimitiveTyped {
            primitive_type,
            value,
        } => {
            write_u8(out, RECORD_MEMBER_PRIMITIVE_TYPED);
            write_u8(out, primitive_type.to_byte());
            write_primitive(out, value)?;
        }
        OutRecord::MemberReference { id_ref } => {
            write_u8(out, RECORD_MEMBER_REFERENCE);
            super::write_i32(out, *id_ref);
        }
        OutRecord::ObjectNull => write_u8(out, RECORD_OBJECT_NULL),
        OutRecord::ObjectNullMultiple256 { null_count } => {
            write_u8(out, RECORD_OBJECT_NULL_MULTIPLE_256);
            write_u8(out, *null_count);
        }
        OutRecord::ObjectNullMultiple { null_count } => {
            write_u8(out, RECORD_OBJECT_NULL_MULTIPLE);
            super::write_i32(out, *null_count);
        }
        OutRecord::BinaryLibrary {
            library_id,
            library_name,
        } => {
            write_u8(out, RECORD_BINARY_LIBRARY);
            super::write_i32(out, *library_id);
            write_length_prefixed_string(out, library_name);
        }
        OutRecord::ArraySinglePrimitive {
            array_info,
            primitive_type,
            values,
        } => {
            write_u8(out, RECORD_ARRAY_SINGLE_PRIMITIVE);
            super::write_i32(out, array_info.object_id);
            super::write_i32(out, array_info.length);
            write_u8(out, primitive_type.to_byte());
            for value in values {
                write_primitive(out, value)?;
            }
        }
        OutRecord::MessageEnd => write_u8(out, RECORD_MESSAGE_END),
    }
    Ok(())
}
