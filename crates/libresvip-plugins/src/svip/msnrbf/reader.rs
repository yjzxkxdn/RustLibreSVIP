//! Port of `libresvip.plugins.svip.msnrbf.svip_reader`.

use std::collections::HashMap;

use libresvip_core::error::Result;

use super::super::xstudio::{self, class, xs_error, DeclType, XsClass, XsField, XsValue};
use super::{
    read_primitive, ArrayElement, ArrayInfo, BinaryArrayRecord, BinaryArrayType, BinaryType,
    BinaryTypeInfo, ByteReader, ClassInfo, ClassRecord, ClassTypeInfo, ClassWithIdRecord,
    MemberTypeInfo, MemberValue, PrimValue, PrimitiveType, Record, Resolved, Stream,
    RECORD_ARRAY_SINGLE_OBJECT, RECORD_ARRAY_SINGLE_PRIMITIVE, RECORD_ARRAY_SINGLE_STRING,
    RECORD_BINARY_ARRAY, RECORD_BINARY_LIBRARY, RECORD_BINARY_OBJECT_STRING,
    RECORD_CLASS_WITH_ID, RECORD_CLASS_WITH_MEMBERS, RECORD_CLASS_WITH_MEMBERS_AND_TYPES,
    RECORD_MEMBER_PRIMITIVE_TYPED, RECORD_MEMBER_REFERENCE, RECORD_MESSAGE_END,
    RECORD_METHOD_CALL, RECORD_METHOD_RETURN, RECORD_OBJECT_NULL,
    RECORD_OBJECT_NULL_MULTIPLE, RECORD_OBJECT_NULL_MULTIPLE_256,
    RECORD_SERIALIZED_STREAM_HEADER, RECORD_SYSTEM_CLASS_WITH_MEMBERS,
    RECORD_SYSTEM_CLASS_WITH_MEMBERS_AND_TYPES,
};

/// A conservative pre-allocation hint: declared counts come from the file and
/// must not be trusted enough to allocate gigabytes up front.
fn capacity_hint(count: usize) -> usize {
    count.min(4096)
}

/// `SVIPFile.parse` — a Pascal `magic`, a Pascal `version` and the record stream.
pub fn read(data: &[u8]) -> Result<Stream> {
    let mut reader = ByteReader::new(data);
    let magic = reader.read_pascal_string()?;
    let version = reader.read_pascal_string()?;
    let mut state = ParseState::default();
    let mut records = Vec::new();
    loop {
        let record = state.parse_record(&mut reader)?;
        let finished = matches!(record, Record::MessageEnd);
        records.push(record);
        if finished {
            break;
        }
    }
    Ok(Stream {
        magic,
        version,
        records,
        classes: state.classes,
        objects: state.objects,
    })
}

#[derive(Clone)]
struct ClassDefHeader {
    class_info: ClassInfo,
    member_types: Option<MemberTypeInfo>,
}

#[derive(Default)]
struct ParseState {
    /// `local_store.classes` as seen while parsing (`ClassWithId` needs it).
    class_defs: HashMap<i32, ClassDefHeader>,
    classes: HashMap<i32, ClassRecord>,
    objects: HashMap<i32, Record>,
}

impl ParseState {
    fn register_class(&mut self, record: &ClassRecord) {
        // Python only wraps `ClassWithMembers`, `SystemClassWithMembersAndTypes`
        // and `ClassWithMembersAndTypes` in `ClassRegistryAdapter`;
        // `SystemClassWithMembers` is deliberately not registered.
        if record.record_type == RECORD_SYSTEM_CLASS_WITH_MEMBERS {
            return;
        }
        self.class_defs.insert(
            record.class_info.object_id,
            ClassDefHeader {
                class_info: record.class_info.clone(),
                member_types: record.member_types.clone(),
            },
        );
        self.classes
            .insert(record.class_info.object_id, record.clone());
    }

    fn register_object(&mut self, object_id: i32, record: &Record) {
        self.objects.insert(object_id, record.clone());
    }

    fn parse_record(&mut self, reader: &mut ByteReader<'_>) -> Result<Record> {
        let record_type = reader.read_u8()?;
        match record_type {
            RECORD_SERIALIZED_STREAM_HEADER => Ok(Record::Header {
                root_id: reader.read_i32()?,
                header_id: reader.read_i32()?,
                major_version: reader.read_i32()?,
                minor_version: reader.read_i32()?,
            }),
            RECORD_CLASS_WITH_ID => {
                let object_id = reader.read_i32()?;
                let metadata_id = reader.read_i32()?;
                let header = self.class_defs.get(&metadata_id).cloned().ok_or_else(|| {
                    xs_error(format!("ClassWithId refers to unknown metadata {metadata_id}"))
                })?;
                let member_count = header.class_info.member_names.len();
                let values =
                    self.parse_member_values(reader, header.member_types.as_ref(), member_count)?;
                let record = Record::ClassWithId(ClassWithIdRecord {
                    object_id,
                    metadata_id,
                    class_info: header.class_info,
                    member_types: header.member_types,
                    values,
                });
                self.register_object(object_id, &record);
                Ok(record)
            }
            RECORD_SYSTEM_CLASS_WITH_MEMBERS => {
                let class_info = parse_class_info(reader)?;
                let member_count = class_info.member_names.len();
                let values = self.parse_member_values(reader, None, member_count)?;
                Ok(Record::Class(ClassRecord {
                    record_type,
                    class_info,
                    member_types: None,
                    library_id: None,
                    values,
                }))
            }
            RECORD_CLASS_WITH_MEMBERS => {
                let class_info = parse_class_info(reader)?;
                let library_id = reader.read_i32()?;
                let member_count = class_info.member_names.len();
                let values = self.parse_member_values(reader, None, member_count)?;
                let record = Record::Class(ClassRecord {
                    record_type,
                    class_info,
                    member_types: None,
                    library_id: Some(library_id),
                    values,
                });
                if let Record::Class(class) = &record {
                    self.register_class(class);
                }
                Ok(record)
            }
            RECORD_SYSTEM_CLASS_WITH_MEMBERS_AND_TYPES => {
                let class_info = parse_class_info(reader)?;
                let member_count = class_info.member_names.len();
                let member_types = parse_member_type_info(reader, member_count)?;
                let values = self.parse_member_values(reader, Some(&member_types), member_count)?;
                let record = Record::Class(ClassRecord {
                    record_type,
                    class_info,
                    member_types: Some(member_types),
                    library_id: None,
                    values,
                });
                if let Record::Class(class) = &record {
                    self.register_class(class);
                }
                Ok(record)
            }
            RECORD_CLASS_WITH_MEMBERS_AND_TYPES => {
                let class_info = parse_class_info(reader)?;
                let member_count = class_info.member_names.len();
                let member_types = parse_member_type_info(reader, member_count)?;
                let library_id = reader.read_i32()?;
                let values = self.parse_member_values(reader, Some(&member_types), member_count)?;
                let record = Record::Class(ClassRecord {
                    record_type,
                    class_info,
                    member_types: Some(member_types),
                    library_id: Some(library_id),
                    values,
                });
                if let Record::Class(class) = &record {
                    self.register_class(class);
                }
                Ok(record)
            }
            RECORD_BINARY_OBJECT_STRING => {
                let object_id = reader.read_i32()?;
                let value = reader.read_length_prefixed_string()?;
                let record = Record::BinaryObjectString { object_id, value };
                self.register_object(object_id, &record);
                Ok(record)
            }
            RECORD_BINARY_ARRAY => {
                let record = self.parse_binary_array(reader)?;
                let object_id = record.object_id;
                let wrapped = Record::BinaryArray(record);
                self.register_object(object_id, &wrapped);
                Ok(wrapped)
            }
            RECORD_MEMBER_PRIMITIVE_TYPED => {
                let primitive_type = PrimitiveType::from_byte(reader.read_u8()?)?;
                let value = read_primitive(reader, primitive_type)?;
                Ok(Record::MemberPrimitiveTyped {
                    primitive_type,
                    value,
                })
            }
            RECORD_MEMBER_REFERENCE => Ok(Record::MemberReference {
                id_ref: reader.read_i32()?,
            }),
            RECORD_OBJECT_NULL => Ok(Record::ObjectNull),
            RECORD_MESSAGE_END => Ok(Record::MessageEnd),
            RECORD_BINARY_LIBRARY => Ok(Record::BinaryLibrary {
                library_id: reader.read_i32()?,
                library_name: reader.read_length_prefixed_string()?,
            }),
            RECORD_OBJECT_NULL_MULTIPLE_256 => Ok(Record::ObjectNullMultiple256 {
                null_count: reader.read_u8()?,
            }),
            RECORD_OBJECT_NULL_MULTIPLE => Ok(Record::ObjectNullMultiple {
                null_count: reader.read_i32()?,
            }),
            RECORD_ARRAY_SINGLE_PRIMITIVE => {
                let array_info = parse_array_info(reader)?;
                let primitive_type = PrimitiveType::from_byte(reader.read_u8()?)?;
                let count = usize::try_from(array_info.length.max(0)).unwrap_or(0);
                let mut values = Vec::with_capacity(capacity_hint(count));
                for _ in 0..count {
                    values.push(read_primitive(reader, primitive_type)?);
                }
                let object_id = array_info.object_id;
                let record = Record::ArraySinglePrimitive {
                    array_info,
                    primitive_type,
                    values,
                };
                self.register_object(object_id, &record);
                Ok(record)
            }
            RECORD_ARRAY_SINGLE_OBJECT | RECORD_ARRAY_SINGLE_STRING => {
                let array_info = parse_array_info(reader)?;
                let count = usize::try_from(array_info.length.max(0)).unwrap_or(0);
                let mut values = Vec::with_capacity(capacity_hint(count));
                for _ in 0..count {
                    values.push(self.parse_record(reader)?);
                }
                let object_id = array_info.object_id;
                let record = if record_type == RECORD_ARRAY_SINGLE_OBJECT {
                    Record::ArraySingleObject { array_info, values }
                } else {
                    Record::ArraySingleString { array_info, values }
                };
                self.register_object(object_id, &record);
                Ok(record)
            }
            RECORD_METHOD_CALL | RECORD_METHOD_RETURN => parse_method(reader, record_type),
            other => Err(xs_error(format!("unknown record type {other}"))),
        }
    }

    fn parse_member_values(
        &mut self,
        reader: &mut ByteReader<'_>,
        member_types: Option<&MemberTypeInfo>,
        member_count: usize,
    ) -> Result<Vec<MemberValue>> {
        match member_types {
            // `MemberValueWithType` — primitives inline, everything else a record.
            Some(member_types) => {
                let mut values = Vec::with_capacity(capacity_hint(member_count));
                for index in 0..member_count {
                    let binary_type = *member_types
                        .binary_types
                        .get(index)
                        .ok_or_else(|| xs_error("member type information is truncated"))?;
                    let info = member_types
                        .additional_infos
                        .get(index)
                        .ok_or_else(|| xs_error("member type information is truncated"))?;
                    if binary_type == BinaryType::Primitive {
                        let BinaryTypeInfo::Primitive(primitive_type) = info else {
                            return Err(xs_error("primitive member without a primitive type"));
                        };
                        let value = read_primitive(reader, *primitive_type)?;
                        values.push(MemberValue::Primitive(*primitive_type, value));
                    } else {
                        values.push(MemberValue::Record(self.parse_record(reader)?));
                    }
                }
                Ok(values)
            }
            // `MemberValue` — one record per member.
            None => {
                let mut values = Vec::with_capacity(capacity_hint(member_count));
                for _ in 0..member_count {
                    values.push(MemberValue::Record(self.parse_record(reader)?));
                }
                Ok(values)
            }
        }
    }

    fn parse_binary_array(&mut self, reader: &mut ByteReader<'_>) -> Result<BinaryArrayRecord> {
        let object_id = reader.read_i32()?;
        let array_type = BinaryArrayType::from_byte(reader.read_u8()?)?;
        let rank = reader.read_i32()?;
        if rank < 0 {
            return Err(xs_error("negative array rank"));
        }
        let mut lengths = Vec::with_capacity(capacity_hint(rank as usize));
        for _ in 0..rank {
            lengths.push(reader.read_i32()?);
        }
        let lower_bounds = if array_type.has_lower_bounds() {
            let mut bounds = Vec::with_capacity(capacity_hint(rank as usize));
            for _ in 0..rank {
                bounds.push(reader.read_i32()?);
            }
            Some(bounds)
        } else {
            None
        };
        let binary_type = BinaryType::from_byte(reader.read_u8()?)?;
        let info = read_binary_type_info(reader, binary_type)?;
        let total = lengths.first().copied().unwrap_or(0);
        let elements = if !array_type.is_rectangular() && rank == 1 && total > 0 {
            let capacity = usize::try_from(total).unwrap_or(0);
            let mut elements: Vec<ArrayElement> = Vec::with_capacity(capacity_hint(capacity));
            let mut flattened: i64 = 0;
            // `RepeatUntil(... len(lst) + sum(null_count - 1) >= total, ...)`.
            while flattened < i64::from(total) {
                let element = self.parse_array_element(reader, binary_type, &info)?;
                flattened += match &element {
                    ArrayElement::Record(Record::ObjectNullMultiple256 { null_count }) => {
                        i64::from(*null_count)
                    }
                    ArrayElement::Record(Record::ObjectNullMultiple { null_count }) => {
                        i64::from(*null_count)
                    }
                    _ => 1,
                };
                elements.push(element);
            }
            Some(elements)
        } else {
            None
        };
        Ok(BinaryArrayRecord {
            object_id,
            array_type,
            rank,
            lengths,
            lower_bounds,
            binary_type,
            info,
            elements,
        })
    }

    fn parse_array_element(
        &mut self,
        reader: &mut ByteReader<'_>,
        binary_type: BinaryType,
        info: &BinaryTypeInfo,
    ) -> Result<ArrayElement> {
        match binary_type {
            BinaryType::Primitive => {
                let BinaryTypeInfo::Primitive(primitive_type) = info else {
                    return Err(xs_error("primitive array element without a primitive type"));
                };
                Ok(ArrayElement::Primitive(
                    *primitive_type,
                    read_primitive(reader, *primitive_type)?,
                ))
            }
            BinaryType::String => Ok(ArrayElement::String(
                reader.read_length_prefixed_string()?,
            )),
            _ => Ok(ArrayElement::Record(self.parse_record(reader)?)),
        }
    }
}

fn parse_class_info(reader: &mut ByteReader<'_>) -> Result<ClassInfo> {
    let object_id = reader.read_i32()?;
    let name = reader.read_length_prefixed_string()?;
    let member_count = reader.read_i32()?;
    if member_count < 0 {
        return Err(xs_error("negative member count"));
    }
    let mut member_names = Vec::with_capacity(capacity_hint(member_count as usize));
    for _ in 0..member_count {
        member_names.push(reader.read_length_prefixed_string()?);
    }
    Ok(ClassInfo {
        object_id,
        name,
        member_names,
    })
}

fn parse_array_info(reader: &mut ByteReader<'_>) -> Result<ArrayInfo> {
    Ok(ArrayInfo {
        object_id: reader.read_i32()?,
        length: reader.read_i32()?,
    })
}

fn read_binary_type_info(
    reader: &mut ByteReader<'_>,
    binary_type: BinaryType,
) -> Result<BinaryTypeInfo> {
    Ok(match binary_type {
        BinaryType::Primitive => {
            BinaryTypeInfo::Primitive(PrimitiveType::from_byte(reader.read_u8()?)?)
        }
        BinaryType::String
        | BinaryType::Object
        | BinaryType::ObjectArray
        | BinaryType::StringArray => BinaryTypeInfo::None,
        BinaryType::SystemClass => {
            BinaryTypeInfo::SystemClass(reader.read_length_prefixed_string()?)
        }
        BinaryType::Class => BinaryTypeInfo::Class(ClassTypeInfo {
            type_name: reader.read_length_prefixed_string()?,
            library_id: reader.read_i32()?,
        }),
        BinaryType::PrimitiveArray => {
            BinaryTypeInfo::PrimitiveArray(PrimitiveType::from_byte(reader.read_u8()?)?)
        }
    })
}

fn parse_member_type_info(
    reader: &mut ByteReader<'_>,
    member_count: usize,
) -> Result<MemberTypeInfo> {
    let mut binary_types = Vec::with_capacity(capacity_hint(member_count));
    for _ in 0..member_count {
        binary_types.push(BinaryType::from_byte(reader.read_u8()?)?);
    }
    let mut additional_infos = Vec::with_capacity(capacity_hint(member_count));
    for binary_type in &binary_types {
        additional_infos.push(read_binary_type_info(reader, *binary_type)?);
    }
    Ok(MemberTypeInfo {
        binary_types,
        additional_infos,
    })
}

fn parse_method(reader: &mut ByteReader<'_>, record_type: u8) -> Result<Record> {
    const ARGS_INLINE: u32 = 0x0000_0002;
    const CONTEXT_INLINE: u32 = 0x0000_0020;
    const RETURN_VALUE_INLINE: u32 = 0x0000_0800;
    let flags = reader.read_u32()?;
    let mut values = Vec::new();
    // `StringValueWithCode` = Const(PrimitiveTypeEnum.String) + LengthPrefixedString.
    fn read_string_value(reader: &mut ByteReader<'_>) -> Result<String> {
        let primitive = PrimitiveType::from_byte(reader.read_u8()?)?;
        if primitive != PrimitiveType::String {
            return Err(xs_error("expected a string value in a method record"));
        }
        reader.read_length_prefixed_string()
    }
    if record_type == RECORD_METHOD_CALL {
        let _method_name = read_string_value(reader)?;
        let _type_name = read_string_value(reader)?;
        if flags & CONTEXT_INLINE != 0 {
            let _context = read_string_value(reader)?;
        }
    } else {
        if flags & RETURN_VALUE_INLINE != 0 {
            let primitive_type = PrimitiveType::from_byte(reader.read_u8()?)?;
            values.push(read_primitive(reader, primitive_type)?);
        }
        if flags & CONTEXT_INLINE != 0 {
            let _context = read_string_value(reader)?;
        }
    }
    if flags & ARGS_INLINE != 0 {
        let count = reader.read_i32()?;
        if count < 0 {
            return Err(xs_error("negative method argument count"));
        }
        for _ in 0..count {
            let primitive_type = PrimitiveType::from_byte(reader.read_u8()?)?;
            values.push(read_primitive(reader, primitive_type)?);
        }
    }
    Ok(Record::Method {
        record_type,
        flags,
        values,
    })
}

// ---------------------------------------------------------------------------
// Building the X Studio model (`SvipReader.build_object` and friends)
// ---------------------------------------------------------------------------

/// `SvipReader.read` after parsing: find the root object and build the model.
pub fn build_app_model(stream: &Stream) -> Result<XsClass> {
    let root_id = stream
        .records
        .iter()
        .find_map(|record| match record {
            Record::Header { root_id, .. } => Some(*root_id),
            _ => None,
        })
        .ok_or_else(|| xs_error("record stream has no SerializedStreamHeader"))?;
    for record in &stream.records {
        let value = match record {
            Record::Class(class) if class.class_info.object_id == root_id => build_class(
                stream,
                &class.class_info,
                class.member_types.as_ref(),
                &class.values,
            )?,
            Record::ClassWithId(with_id) if with_id.class_info.object_id == root_id => {
                build_class(
                    stream,
                    &with_id.class_info,
                    with_id.member_types.as_ref(),
                    &with_id.values,
                )?
            }
            _ => continue,
        };
        if value.name == class::APP_MODEL {
            return Ok(value);
        }
    }
    Err(xs_error("Root not found"))
}

fn build_class(
    stream: &Stream,
    class_info: &ClassInfo,
    _member_types: Option<&MemberTypeInfo>,
    values: &[MemberValue],
) -> Result<XsClass> {
    let full_name = class_info.name.as_str();
    let class_name = full_name.split("`1").next().unwrap_or(full_name);
    let static_name = xstudio::static_class_name(class_name)
        .ok_or_else(|| xs_error(format!("unknown X Studio class {class_name}")))?;
    let table = xstudio::alias_table(class_name)
        .ok_or_else(|| xs_error(format!("unknown X Studio class {class_name}")))?;
    if class_info.member_names.len() != values.len() {
        return Err(xs_error(format!(
            "{class_name} declares {} members but carries {} values",
            class_info.member_names.len(),
            values.len()
        )));
    }
    let mut fields = Vec::with_capacity(values.len());
    for (alias, member) in class_info.member_names.iter().zip(values.iter()) {
        let (field_alias, field_name) = table
            .iter()
            .find(|(candidate, _)| candidate == alias)
            .ok_or_else(|| xs_error(format!("{class_name} has no member named {alias}")))?;
        fields.push(XsField::new(
            field_name,
            field_alias,
            DeclType::Unknown,
            build_member_value(stream, member)?,
        ));
    }
    let mut result = XsClass::new(static_name, fields);
    if static_name == class::GENERIC_LIST {
        // `class_kwargs["items"] = class_kwargs["items"][: class_kwargs["size"]]`
        let size = result.i64("size")?;
        let items = result
            .fields
            .iter_mut()
            .find(|field| field.name == "items")
            .ok_or_else(|| xs_error("System.Collections.Generic.List has no _items"))?;
        let XsValue::List(items) = &mut items.value else {
            return Err(xs_error(
                "System.Collections.Generic.List::_items is not a list",
            ));
        };
        let length = items.len();
        let keep = if size >= 0 {
            (size as usize).min(length)
        } else {
            length.saturating_sub(size.unsigned_abs() as usize)
        };
        items.truncate(keep);
    }
    Ok(result)
}

fn build_member_value(stream: &Stream, member: &MemberValue) -> Result<XsValue> {
    match member {
        MemberValue::Primitive(_, value) => Ok(prim_to_xs(value)),
        MemberValue::Record(record) => build_record(stream, record),
    }
}

fn build_record(stream: &Stream, record: &Record) -> Result<XsValue> {
    match record {
        Record::BinaryObjectString { value, .. } => Ok(XsValue::Str(value.clone())),
        Record::BinaryArray(array) => build_binary_array(stream, array),
        Record::Class(class) => Ok(XsValue::Class(Box::new(build_class(
            stream,
            &class.class_info,
            class.member_types.as_ref(),
            &class.values,
        )?))),
        Record::ClassWithId(with_id) => Ok(XsValue::Class(Box::new(build_class(
            stream,
            &with_id.class_info,
            with_id.member_types.as_ref(),
            &with_id.values,
        )?))),
        Record::MemberReference { id_ref } => build_reference(stream, *id_ref),
        Record::MemberPrimitiveTyped { value, .. } => Ok(prim_to_xs(value)),
        Record::ObjectNull => Ok(XsValue::Null),
        Record::ObjectNullMultiple256 { null_count } => {
            Ok(XsValue::List(vec![XsValue::Null; usize::from(*null_count)]))
        }
        Record::ObjectNullMultiple { null_count } => {
            Ok(XsValue::List(vec![
                XsValue::Null;
                checked_null_count(*null_count)?
            ]))
        }
        Record::ArraySinglePrimitive {
            primitive_type,
            values,
            ..
        } => {
            if *primitive_type == PrimitiveType::Byte {
                let mut bytes = Vec::with_capacity(values.len());
                for value in values {
                    match value {
                        PrimValue::Byte(byte) => bytes.push(*byte),
                        other => {
                            return Err(xs_error(format!(
                                "expected a Byte array element, found {other:?}"
                            )))
                        }
                    }
                }
                Ok(XsValue::Bytes(bytes))
            } else {
                // Python logs a warning and returns `None`.
                Ok(XsValue::Null)
            }
        }
        // Python logs a warning and returns `None` for every other record type.
        _ => Ok(XsValue::Null),
    }
}

fn build_reference(stream: &Stream, id_ref: i32) -> Result<XsValue> {
    match stream.resolve(id_ref)? {
        Resolved::Class(class) => Ok(XsValue::Class(Box::new(build_class(
            stream,
            &class.class_info,
            class.member_types.as_ref(),
            &class.values,
        )?))),
        Resolved::Object(object) => build_record(stream, object),
    }
}

fn build_binary_array(stream: &Stream, array: &BinaryArrayRecord) -> Result<XsValue> {
    if !array.binary_type.contains_class() {
        // `logger.warning(obj.binary_type_enum)` — result stays empty.
        return Ok(XsValue::List(Vec::new()));
    }
    let Some(elements) = &array.elements else {
        return Ok(XsValue::List(vec![
            XsValue::Null;
            checked_null_count(array.total_length())?
        ]));
    };
    let mut results = Vec::with_capacity(elements.len());
    for element in elements {
        match element {
            ArrayElement::Primitive(_, value) => results.push(prim_to_xs(value)),
            ArrayElement::String(value) => results.push(XsValue::Str(value.clone())),
            ArrayElement::Record(Record::ObjectNull) => results.push(XsValue::Null),
            ArrayElement::Record(Record::ObjectNullMultiple256 { null_count }) => {
                results.extend(std::iter::repeat(XsValue::Null).take(usize::from(*null_count)));
            }
            ArrayElement::Record(Record::ObjectNullMultiple { null_count }) => {
                results.extend(
                    std::iter::repeat(XsValue::Null).take(checked_null_count(*null_count)?),
                );
            }
            ArrayElement::Record(record) => results.push(build_record(stream, record)?),
        }
    }
    Ok(XsValue::List(results))
}

/// `[None] * count` guard: a handful of bytes can claim billions of nulls, which
/// Python would try to allocate. Malformed input becomes an error instead.
fn checked_null_count(count: i32) -> Result<usize> {
    const MAX_NULL_FILL: i32 = 1 << 20;
    if count < 0 || count > MAX_NULL_FILL {
        return Err(xs_error(format!("implausible null array length {count}")));
    }
    Ok(count as usize)
}

/// `build_object` primitives → the generic value tree.
pub fn prim_to_xs(value: &PrimValue) -> XsValue {
    match value {
        PrimValue::Boolean(value) => XsValue::Bool(*value),
        PrimValue::Byte(value) => XsValue::Int(i64::from(*value)),
        PrimValue::Char(value) => XsValue::Str(value.clone()),
        PrimValue::Decimal(value) => XsValue::Str(value.clone()),
        PrimValue::Double(value) => XsValue::F64(*value),
        PrimValue::Int16(value) => XsValue::Int(i64::from(*value)),
        PrimValue::Int32(value) => XsValue::Int(i64::from(*value)),
        PrimValue::Int64(value) => XsValue::Int(*value),
        PrimValue::SByte(value) => XsValue::Int(i64::from(*value)),
        PrimValue::Single(value) => XsValue::F32(*value),
        PrimValue::TimeSpan(value) => XsValue::Int(*value as i64),
        PrimValue::DateTime(value) => XsValue::Int(*value as i64),
        PrimValue::UInt16(value) => XsValue::Int(i64::from(*value)),
        PrimValue::UInt32(value) => XsValue::Int(i64::from(*value)),
        PrimValue::UInt64(value) => XsValue::Int(*value as i64),
        PrimValue::Null => XsValue::Null,
        PrimValue::String(value) => XsValue::Str(value.clone()),
    }
}
