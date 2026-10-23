//! Port of `libresvip.plugins.svip.msnrbf.binary_models`.
//!
//! MS-NRBF (the `BinaryFormatter` remoting format) record stream used by X
//! Studio `.svip` files. The Python implementation builds the record stream with
//! `construct`; here the same structures are modelled as Rust enums plus two
//! small byte codecs.
//!
//! The public surface is [`Stream`] (a parsed record stream, with the class /
//! object registries the Python `local_store` keeps) for reading, and
//! [`writer::SvipWriter`] for writing.

use std::collections::HashMap;

use libresvip_core::error::Result;

use super::xstudio::xs_error;

pub mod reader;
pub mod writer;

// Record type enums (MS-NRBF 2.1.2.1).
pub const RECORD_SERIALIZED_STREAM_HEADER: u8 = 0;
pub const RECORD_CLASS_WITH_ID: u8 = 1;
pub const RECORD_SYSTEM_CLASS_WITH_MEMBERS: u8 = 2;
pub const RECORD_CLASS_WITH_MEMBERS: u8 = 3;
pub const RECORD_SYSTEM_CLASS_WITH_MEMBERS_AND_TYPES: u8 = 4;
pub const RECORD_CLASS_WITH_MEMBERS_AND_TYPES: u8 = 5;
pub const RECORD_BINARY_OBJECT_STRING: u8 = 6;
pub const RECORD_BINARY_ARRAY: u8 = 7;
pub const RECORD_MEMBER_PRIMITIVE_TYPED: u8 = 8;
pub const RECORD_MEMBER_REFERENCE: u8 = 9;
pub const RECORD_OBJECT_NULL: u8 = 10;
pub const RECORD_MESSAGE_END: u8 = 11;
pub const RECORD_BINARY_LIBRARY: u8 = 12;
pub const RECORD_OBJECT_NULL_MULTIPLE_256: u8 = 13;
pub const RECORD_OBJECT_NULL_MULTIPLE: u8 = 14;
pub const RECORD_ARRAY_SINGLE_PRIMITIVE: u8 = 15;
pub const RECORD_ARRAY_SINGLE_OBJECT: u8 = 16;
pub const RECORD_ARRAY_SINGLE_STRING: u8 = 17;
pub const RECORD_METHOD_CALL: u8 = 21;
pub const RECORD_METHOD_RETURN: u8 = 22;

/// `PrimitiveTypeEnum`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimitiveType {
    Boolean,
    Byte,
    Char,
    Decimal,
    Double,
    Int16,
    Int32,
    Int64,
    SByte,
    Single,
    TimeSpan,
    DateTime,
    UInt16,
    UInt32,
    UInt64,
    Null,
    String,
}

impl PrimitiveType {
    pub fn from_byte(value: u8) -> Result<Self> {
        Ok(match value {
            1 => Self::Boolean,
            2 => Self::Byte,
            3 => Self::Char,
            5 => Self::Decimal,
            6 => Self::Double,
            7 => Self::Int16,
            8 => Self::Int32,
            9 => Self::Int64,
            10 => Self::SByte,
            11 => Self::Single,
            12 => Self::TimeSpan,
            13 => Self::DateTime,
            14 => Self::UInt16,
            15 => Self::UInt32,
            16 => Self::UInt64,
            17 => Self::Null,
            18 => Self::String,
            other => return Err(xs_error(format!("invalid primitive type {other}"))),
        })
    }

    pub fn to_byte(self) -> u8 {
        match self {
            Self::Boolean => 1,
            Self::Byte => 2,
            Self::Char => 3,
            Self::Decimal => 5,
            Self::Double => 6,
            Self::Int16 => 7,
            Self::Int32 => 8,
            Self::Int64 => 9,
            Self::SByte => 10,
            Self::Single => 11,
            Self::TimeSpan => 12,
            Self::DateTime => 13,
            Self::UInt16 => 14,
            Self::UInt32 => 15,
            Self::UInt64 => 16,
            Self::Null => 17,
            Self::String => 18,
        }
    }
}

/// `BinaryTypeEnum`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryType {
    Primitive,
    String,
    Object,
    SystemClass,
    Class,
    ObjectArray,
    StringArray,
    PrimitiveArray,
}

impl BinaryType {
    pub fn from_byte(value: u8) -> Result<Self> {
        Ok(match value {
            0 => Self::Primitive,
            1 => Self::String,
            2 => Self::Object,
            3 => Self::SystemClass,
            4 => Self::Class,
            5 => Self::ObjectArray,
            6 => Self::StringArray,
            7 => Self::PrimitiveArray,
            other => return Err(xs_error(format!("invalid binary type {other}"))),
        })
    }

    pub fn to_byte(self) -> u8 {
        match self {
            Self::Primitive => 0,
            Self::String => 1,
            Self::Object => 2,
            Self::SystemClass => 3,
            Self::Class => 4,
            Self::ObjectArray => 5,
            Self::StringArray => 6,
            Self::PrimitiveArray => 7,
        }
    }

    /// `"Class" in str(binary_type_enum)` — true for `Class` and `SystemClass`.
    pub fn contains_class(self) -> bool {
        matches!(self, Self::Class | Self::SystemClass)
    }
}

/// `BinaryArrayTypeEnum`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryArrayType {
    Single,
    Jagged,
    Rectangular,
    SingleOffset,
    JaggedOffset,
    RectangularOffset,
}

impl BinaryArrayType {
    pub fn from_byte(value: u8) -> Result<Self> {
        Ok(match value {
            0 => Self::Single,
            1 => Self::Jagged,
            2 => Self::Rectangular,
            3 => Self::SingleOffset,
            4 => Self::JaggedOffset,
            5 => Self::RectangularOffset,
            other => return Err(xs_error(format!("invalid binary array type {other}"))),
        })
    }

    pub fn to_byte(self) -> u8 {
        match self {
            Self::Single => 0,
            Self::Jagged => 1,
            Self::Rectangular => 2,
            Self::SingleOffset => 3,
            Self::JaggedOffset => 4,
            Self::RectangularOffset => 5,
        }
    }

    pub fn has_lower_bounds(self) -> bool {
        matches!(
            self,
            Self::SingleOffset | Self::JaggedOffset | Self::RectangularOffset
        )
    }

    pub fn is_rectangular(self) -> bool {
        matches!(self, Self::Rectangular | Self::RectangularOffset)
    }
}

/// A decoded primitive value (`PrimitiveType` + payload).
#[derive(Clone, Debug, PartialEq)]
pub enum PrimValue {
    Boolean(bool),
    Byte(u8),
    Char(String),
    Decimal(String),
    Double(f64),
    Int16(i16),
    Int32(i32),
    Int64(i64),
    SByte(i8),
    Single(f32),
    TimeSpan(u64),
    DateTime(u64),
    UInt16(u16),
    UInt32(u32),
    UInt64(u64),
    Null,
    String(String),
}

impl PrimValue {
    pub fn is_null(&self) -> bool {
        matches!(self, PrimValue::Null)
    }
}

/// `ArrayInfo`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArrayInfo {
    pub object_id: i32,
    pub length: i32,
}

/// `ClassInfo`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassInfo {
    pub object_id: i32,
    pub name: String,
    pub member_names: Vec<String>,
}

/// `ClassTypeInfo`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassTypeInfo {
    pub type_name: String,
    pub library_id: i32,
}

/// The additional information part of a binary type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BinaryTypeInfo {
    /// `Null` — `String`, `Object`, `ObjectArray`, `StringArray`.
    None,
    Primitive(PrimitiveType),
    SystemClass(String),
    Class(ClassTypeInfo),
    PrimitiveArray(PrimitiveType),
}

/// `MemberTypeInfo`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MemberTypeInfo {
    pub binary_types: Vec<BinaryType>,
    pub additional_infos: Vec<BinaryTypeInfo>,
}



/// `MemberValueWithType` / `MemberValue`.
#[derive(Clone, Debug)]
pub enum MemberValue {
    Primitive(PrimitiveType, PrimValue),
    Record(Record),
}

/// One element of a `BinaryArray`.
#[derive(Clone, Debug)]
pub enum ArrayElement {
    Primitive(PrimitiveType, PrimValue),
    String(String),
    Record(Record),
}

/// A parsed NRBF record.
#[derive(Clone, Debug)]
pub enum Record {
    Header {
        root_id: i32,
        header_id: i32,
        major_version: i32,
        minor_version: i32,
    },
    Class(ClassRecord),
    ClassWithId(ClassWithIdRecord),
    BinaryObjectString {
        object_id: i32,
        value: String,
    },
    BinaryArray(BinaryArrayRecord),
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
    ArraySingleObject {
        array_info: ArrayInfo,
        values: Vec<Record>,
    },
    ArraySingleString {
        array_info: ArrayInfo,
        values: Vec<Record>,
    },
    /// `MethodCall` / `MethodReturn` — parsed but not interpreted.
    Method {
        record_type: u8,
        flags: u32,
        values: Vec<PrimValue>,
    },
    MessageEnd,
}

impl Record {
    pub fn record_type(&self) -> u8 {
        match self {
            Record::Header { .. } => RECORD_SERIALIZED_STREAM_HEADER,
            Record::Class(class) => class.record_type,
            Record::ClassWithId(_) => RECORD_CLASS_WITH_ID,
            Record::BinaryObjectString { .. } => RECORD_BINARY_OBJECT_STRING,
            Record::BinaryArray(_) => RECORD_BINARY_ARRAY,
            Record::MemberPrimitiveTyped { .. } => RECORD_MEMBER_PRIMITIVE_TYPED,
            Record::MemberReference { .. } => RECORD_MEMBER_REFERENCE,
            Record::ObjectNull => RECORD_OBJECT_NULL,
            Record::ObjectNullMultiple256 { .. } => RECORD_OBJECT_NULL_MULTIPLE_256,
            Record::ObjectNullMultiple { .. } => RECORD_OBJECT_NULL_MULTIPLE,
            Record::BinaryLibrary { .. } => RECORD_BINARY_LIBRARY,
            Record::ArraySinglePrimitive { .. } => RECORD_ARRAY_SINGLE_PRIMITIVE,
            Record::ArraySingleObject { .. } => RECORD_ARRAY_SINGLE_OBJECT,
            Record::ArraySingleString { .. } => RECORD_ARRAY_SINGLE_STRING,
            Record::Method { record_type, .. } => *record_type,
            Record::MessageEnd => RECORD_MESSAGE_END,
        }
    }

    /// `"Class" in str(record_type_enum)`.
    pub fn is_class_like(&self) -> bool {
        matches!(self, Record::Class(_) | Record::ClassWithId(_))
    }
}

/// `ClassWithMembersAndTypes` / `SystemClassWithMembersAndTypes` /
/// `ClassWithMembers` / `SystemClassWithMembers`.
#[derive(Clone, Debug)]
pub struct ClassRecord {
    pub record_type: u8,
    pub class_info: ClassInfo,
    pub member_types: Option<MemberTypeInfo>,
    pub library_id: Option<i32>,
    pub values: Vec<MemberValue>,
}

/// `ClassWithId`.
#[derive(Clone, Debug)]
pub struct ClassWithIdRecord {
    pub object_id: i32,
    pub metadata_id: i32,
    /// `Computed(lambda this: local_store.classes[this.metadata_id]["class_info"])`.
    pub class_info: ClassInfo,
    pub member_types: Option<MemberTypeInfo>,
    pub values: Vec<MemberValue>,
}

/// `BinaryArray`.
#[derive(Clone, Debug)]
pub struct BinaryArrayRecord {
    pub object_id: i32,
    pub array_type: BinaryArrayType,
    pub rank: i32,
    pub lengths: Vec<i32>,
    pub lower_bounds: Option<Vec<i32>>,
    pub binary_type: BinaryType,
    pub info: BinaryTypeInfo,
    /// `None` when the record has no member values (`If(...)` in Python).
    pub elements: Option<Vec<ArrayElement>>,
}

impl BinaryArrayRecord {
    pub fn total_length(&self) -> i32 {
        self.lengths.first().copied().unwrap_or(0)
    }
}

/// A parsed `.svip` record stream plus the registries `local_store` holds.
pub struct Stream {
    pub magic: String,
    pub version: String,
    pub records: Vec<Record>,
    /// `local_store.classes` — class definitions by `object_id`.
    pub classes: HashMap<i32, ClassRecord>,
    /// `local_store.objects` — objects by `object_id`.
    pub objects: HashMap<i32, Record>,
}

impl Stream {
    pub fn full_version(&self) -> String {
        format!("{}{}", self.magic, self.version)
    }

    /// `ChainMap(local_store.classes, local_store.objects)[id]`.
    pub fn resolve(&self, object_id: i32) -> Result<Resolved<'_>> {
        if let Some(class) = self.classes.get(&object_id) {
            return Ok(Resolved::Class(class));
        }
        if let Some(object) = self.objects.get(&object_id) {
            return Ok(Resolved::Object(object));
        }
        Err(xs_error(format!(
            "member reference {object_id} does not point at a known object"
        )))
    }
}

/// The target of a resolved `MemberReference`.
pub enum Resolved<'a> {
    Class(&'a ClassRecord),
    Object(&'a Record),
}

// ---------------------------------------------------------------------------
// Byte codec helpers
// ---------------------------------------------------------------------------

pub struct ByteReader<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> ByteReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }

    pub fn position(&self) -> usize {
        self.position
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.position)
    }

    pub fn take(&mut self, length: usize) -> Result<&'a [u8]> {
        let end = self
            .position
            .checked_add(length)
            .ok_or_else(|| xs_error("record stream offset overflow"))?;
        if end > self.data.len() {
            return Err(xs_error("unexpected end of record stream"));
        }
        let slice = &self.data[self.position..end];
        self.position = end;
        Ok(slice)
    }

    /// A fixed-size field; the length is part of the type, so a short read is a
    /// malformed file, never a panic.
    pub fn read_array<const N: usize>(&mut self) -> Result<[u8; N]> {
        let slice = self.take(N)?;
        <[u8; N]>::try_from(slice).map_err(|_| xs_error("unexpected end of record stream"))
    }

    pub fn read_u8(&mut self) -> Result<u8> {
        Ok(self.read_array::<1>()?[0])
    }

    pub fn read_i8(&mut self) -> Result<i8> {
        Ok(self.read_u8()? as i8)
    }

    pub fn read_u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.read_array::<2>()?))
    }

    pub fn read_i16(&mut self) -> Result<i16> {
        Ok(i16::from_le_bytes(self.read_array::<2>()?))
    }

    pub fn read_u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.read_array::<4>()?))
    }

    pub fn read_i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.read_array::<4>()?))
    }

    pub fn read_u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.read_array::<8>()?))
    }

    pub fn read_i64(&mut self) -> Result<i64> {
        Ok(i64::from_le_bytes(self.read_array::<8>()?))
    }

    pub fn read_f32(&mut self) -> Result<f32> {
        Ok(f32::from_bits(self.read_u32()?))
    }

    pub fn read_f64(&mut self) -> Result<f64> {
        Ok(f64::from_bits(self.read_u64()?))
    }

    /// `PascalString(Byte, "utf-8")`.
    pub fn read_pascal_string(&mut self) -> Result<String> {
        let length = self.read_u8()? as usize;
        let bytes = self.take(length)?;
        String::from_utf8(bytes.to_vec())
            .map_err(|_| xs_error("invalid UTF-8 in a length-prefixed string"))
    }

    /// `LengthPrefixedString`.
    pub fn read_length_prefixed_string(&mut self) -> Result<String> {
        let mut length = 0usize;
        let mut shift = 0u32;
        let mut terminated = false;
        for _ in 0..5 {
            let byte = self.read_u8()?;
            length |= ((byte & 0x7F) as usize)
                .checked_shl(shift)
                .ok_or_else(|| xs_error("length-prefixed string is too long"))?;
            shift += 7;
            if byte & 0x80 == 0 {
                terminated = true;
                break;
            }
        }
        if !terminated {
            return Err(xs_error("invalid length-prefixed string"));
        }
        let bytes = self.take(length)?;
        String::from_utf8(bytes.to_vec())
            .map_err(|_| xs_error("invalid UTF-8 in a length-prefixed string"))
    }

    /// `Utf8CodePoint` — a bare UTF-8 code point (no length prefix).
    pub fn read_utf8_char(&mut self) -> Result<String> {
        let first = self.read_u8()?;
        let length = if first & 0x80 == 0 {
            1
        } else if first & 0xE0 == 0xC0 {
            2
        } else if first & 0xF0 == 0xE0 {
            3
        } else if first & 0xF8 == 0xF0 {
            4
        } else {
            return Err(xs_error("invalid UTF-8 code point"));
        };
        let mut bytes = vec![first];
        if length > 1 {
            bytes.extend_from_slice(self.take(length - 1)?);
        }
        String::from_utf8(bytes).map_err(|_| xs_error("invalid UTF-8 code point"))
    }
}

pub fn write_u8(out: &mut Vec<u8>, value: u8) {
    out.push(value);
}

pub fn write_i8(out: &mut Vec<u8>, value: i8) {
    out.push(value as u8);
}

pub fn write_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

pub fn write_i16(out: &mut Vec<u8>, value: i16) {
    out.extend_from_slice(&value.to_le_bytes());
}

pub fn write_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

pub fn write_i32(out: &mut Vec<u8>, value: i32) {
    out.extend_from_slice(&value.to_le_bytes());
}

pub fn write_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes());
}

pub fn write_i64(out: &mut Vec<u8>, value: i64) {
    out.extend_from_slice(&value.to_le_bytes());
}

pub fn write_f32(out: &mut Vec<u8>, value: f32) {
    out.extend_from_slice(&value.to_le_bytes());
}

pub fn write_f64(out: &mut Vec<u8>, value: f64) {
    out.extend_from_slice(&value.to_le_bytes());
}

pub fn write_pascal_string(out: &mut Vec<u8>, value: &str) -> Result<()> {
    let bytes = value.as_bytes();
    let length = u8::try_from(bytes.len())
        .map_err(|_| xs_error("pascal string is longer than 255 bytes"))?;
    out.push(length);
    out.extend_from_slice(bytes);
    Ok(())
}

pub fn write_length_prefixed_string(out: &mut Vec<u8>, value: &str) {
    let mut length = value.len();
    while length > 0x7F {
        out.push(((length & 0x7F) | 0x80) as u8);
        length >>= 7;
    }
    out.push(length as u8);
    out.extend_from_slice(value.as_bytes());
}

/// `TimeSpanAdapter` / `Int64ul` encoding of a tick count.
pub fn if_tick_count_encoding(ticks: i64) -> u64 {
    ticks as u64
}

/// `PrimitiveType(...)` — one primitive value of the given type.
pub fn read_primitive(
    reader: &mut ByteReader<'_>,
    primitive_type: PrimitiveType,
) -> Result<PrimValue> {
    Ok(match primitive_type {
        PrimitiveType::Boolean => PrimValue::Boolean(reader.read_u8()? != 0),
        PrimitiveType::Byte => PrimValue::Byte(reader.read_u8()?),
        PrimitiveType::Char => PrimValue::Char(reader.read_utf8_char()?),
        PrimitiveType::Decimal => PrimValue::Decimal(reader.read_length_prefixed_string()?),
        PrimitiveType::Double => PrimValue::Double(reader.read_f64()?),
        PrimitiveType::Int16 => PrimValue::Int16(reader.read_i16()?),
        PrimitiveType::Int32 => PrimValue::Int32(reader.read_i32()?),
        PrimitiveType::Int64 => PrimValue::Int64(reader.read_i64()?),
        PrimitiveType::SByte => PrimValue::SByte(reader.read_i8()?),
        PrimitiveType::Single => PrimValue::Single(reader.read_f32()?),
        PrimitiveType::TimeSpan => PrimValue::TimeSpan(reader.read_u64()?),
        PrimitiveType::DateTime => {
            // `BitStruct(ticks=BitsInteger(62), kind=BitsInteger(2))` reads the
            // bit fields most-significant-bit first.
            PrimValue::DateTime(u64::from_be_bytes(reader.read_array::<8>()?))
        }
        PrimitiveType::UInt16 => PrimValue::UInt16(reader.read_u16()?),
        PrimitiveType::UInt32 => PrimValue::UInt32(reader.read_u32()?),
        PrimitiveType::UInt64 => PrimValue::UInt64(reader.read_u64()?),
        PrimitiveType::Null => PrimValue::Null,
        PrimitiveType::String => PrimValue::String(reader.read_length_prefixed_string()?),
    })
}

/// Serialises a primitive value exactly like the `construct` subcon of its type.
pub fn write_primitive(out: &mut Vec<u8>, value: &PrimValue) -> Result<()> {
    match value {
        PrimValue::Boolean(value) => out.push(u8::from(*value)),
        PrimValue::Byte(value) => write_u8(out, *value),
        PrimValue::Char(value) => out.extend_from_slice(value.as_bytes()),
        PrimValue::Decimal(value) => write_length_prefixed_string(out, value),
        PrimValue::Double(value) => write_f64(out, *value),
        PrimValue::Int16(value) => write_i16(out, *value),
        PrimValue::Int32(value) => write_i32(out, *value),
        PrimValue::Int64(value) => write_i64(out, *value),
        PrimValue::SByte(value) => write_i8(out, *value),
        PrimValue::Single(value) => write_f32(out, *value),
        PrimValue::TimeSpan(value) => write_u64(out, *value),
        PrimValue::DateTime(value) => out.extend_from_slice(&value.to_be_bytes()),
        PrimValue::UInt16(value) => write_u16(out, *value),
        PrimValue::UInt32(value) => write_u32(out, *value),
        PrimValue::UInt64(value) => write_u64(out, *value),
        PrimValue::Null => {}
        PrimValue::String(value) => write_length_prefixed_string(out, value),
    }
    Ok(())
}
