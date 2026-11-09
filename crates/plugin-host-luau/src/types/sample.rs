//! A value's shape as serde serializes it, for a type that no deserializer
//! traces, as one tagged inside: of a value of each of its variants.

use serde::ser::{self, Impossible, Serialize};

use super::trace::{Format, Shape, Stop, Tracer};

impl ser::Error for Stop {
    fn custom<T: std::fmt::Display>(message: T) -> Self {
        Self::Other(message.to_string())
    }
}

/// What a value serialized as: its format, its text, of a text, and its
/// fields, of a struct, by their names as serde gives them.
#[derive(Debug)]
pub(crate) struct Described {
    pub(crate) format: Format,
    pub(crate) text: Option<String>,
    pub(crate) fields: Vec<(&'static str, Described)>,
}

impl Described {
    fn of(format: Format) -> Self {
        Self {
            format,
            text: None,
            fields: Vec::new(),
        }
    }
}

/// What `value` serializes as, its enums' and newtypes' names of
/// `tracer`'s.
pub(crate) fn describe<T: Serialize>(value: &T, tracer: &Tracer) -> Result<Described, Stop> {
    value.serialize(Shaping { tracer })
}

/// A serializer that tells what a value is.
#[derive(Clone, Copy)]
struct Shaping<'t> {
    tracer: &'t Tracer,
}

impl<'t> ser::Serializer for Shaping<'t> {
    type Ok = Described;
    type Error = Stop;
    type SerializeSeq = Items<'t>;
    type SerializeTuple = Items<'t>;
    type SerializeTupleStruct = Items<'t>;
    type SerializeTupleVariant = Impossible<Described, Stop>;
    type SerializeMap = Impossible<Described, Stop>;
    type SerializeStruct = Fields<'t>;
    type SerializeStructVariant = Impossible<Described, Stop>;

    fn serialize_bool(self, _: bool) -> Result<Described, Stop> {
        Ok(Described::of(Format::Bool))
    }

    fn serialize_i8(self, _: i8) -> Result<Described, Stop> {
        Ok(Described::of(Format::Number))
    }

    fn serialize_i16(self, _: i16) -> Result<Described, Stop> {
        Ok(Described::of(Format::Number))
    }

    fn serialize_i32(self, _: i32) -> Result<Described, Stop> {
        Ok(Described::of(Format::Number))
    }

    fn serialize_i64(self, _: i64) -> Result<Described, Stop> {
        Ok(Described::of(Format::Number))
    }

    fn serialize_u8(self, _: u8) -> Result<Described, Stop> {
        Ok(Described::of(Format::Number))
    }

    fn serialize_u16(self, _: u16) -> Result<Described, Stop> {
        Ok(Described::of(Format::Number))
    }

    fn serialize_u32(self, _: u32) -> Result<Described, Stop> {
        Ok(Described::of(Format::Number))
    }

    fn serialize_u64(self, _: u64) -> Result<Described, Stop> {
        Ok(Described::of(Format::Number))
    }

    fn serialize_f32(self, _: f32) -> Result<Described, Stop> {
        Ok(Described::of(Format::Number))
    }

    fn serialize_f64(self, _: f64) -> Result<Described, Stop> {
        Ok(Described::of(Format::Number))
    }

    fn serialize_char(self, _: char) -> Result<Described, Stop> {
        Ok(Described::of(Format::Text))
    }

    fn serialize_str(self, text: &str) -> Result<Described, Stop> {
        Ok(Described {
            text: Some(text.to_owned()),
            ..Described::of(Format::Text)
        })
    }

    fn serialize_bytes(self, _: &[u8]) -> Result<Described, Stop> {
        Ok(Described::of(Format::Bytes))
    }

    fn serialize_none(self) -> Result<Described, Stop> {
        Ok(Described::of(Format::Option(Box::new(Format::Unknown))))
    }

    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<Described, Stop> {
        let inner = value.serialize(self)?;
        Ok(Described::of(Format::Option(Box::new(inner.format))))
    }

    fn serialize_unit(self) -> Result<Described, Stop> {
        Ok(Described::of(Format::Unit))
    }

    fn serialize_unit_struct(self, _: &'static str) -> Result<Described, Stop> {
        Ok(Described::of(Format::Unit))
    }

    fn serialize_unit_variant(
        self,
        name: &'static str,
        _: u32,
        variant: &'static str,
    ) -> Result<Described, Stop> {
        // The enum traced of the name that holds the variant.
        let shapes = self.tracer.shapes.borrow();
        let traced = shapes.iter().find_map(|(id, shape)| {
            (id.name == name && matches!(shape, Shape::Enum(_)) && id.members.contains(&variant))
                .then_some(*id)
        });
        Ok(Described::of(traced.map_or(Format::Text, Format::Named)))
    }

    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        name: &'static str,
        value: &T,
    ) -> Result<Described, Stop> {
        let inner = value.serialize(self)?;
        self.tracer
            .newtypes
            .borrow_mut()
            .entry(name)
            .or_insert(inner.format);
        Ok(Described::of(Format::Newtype(name)))
    }

    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        name: &'static str,
        _: u32,
        _: &'static str,
        _: &T,
    ) -> Result<Described, Stop> {
        Err(Stop::Other(format!("{name}'s variant of a value")))
    }

    fn serialize_seq(self, _: Option<usize>) -> Result<Items<'t>, Stop> {
        Ok(Items {
            shaping: self,
            items: Vec::new(),
            list: true,
        })
    }

    fn serialize_tuple(self, _: usize) -> Result<Items<'t>, Stop> {
        Ok(Items {
            shaping: self,
            items: Vec::new(),
            list: false,
        })
    }

    fn serialize_tuple_struct(self, _: &'static str, length: usize) -> Result<Items<'t>, Stop> {
        self.serialize_tuple(length)
    }

    fn serialize_tuple_variant(
        self,
        name: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleVariant, Stop> {
        Err(Stop::Other(format!("{name}'s variant of values")))
    }

    fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, Stop> {
        Err(Stop::Other("a map".to_owned()))
    }

    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Fields<'t>, Stop> {
        Ok(Fields {
            shaping: self,
            fields: Vec::new(),
        })
    }

    fn serialize_struct_variant(
        self,
        name: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStructVariant, Stop> {
        Err(Stop::Other(format!("{name}'s variant of fields")))
    }
}

/// A list's items, or a tuple's.
struct Items<'t> {
    shaping: Shaping<'t>,
    items: Vec<Format>,
    list: bool,
}

impl Items<'_> {
    fn push<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Stop> {
        self.items.push(value.serialize(self.shaping)?.format);
        Ok(())
    }

    fn end(self) -> Described {
        if self.list {
            let item = self.items.into_iter().next().unwrap_or(Format::Unknown);
            Described::of(Format::List(Box::new(item)))
        } else {
            Described::of(Format::Tuple(self.items))
        }
    }
}

impl ser::SerializeSeq for Items<'_> {
    type Ok = Described;
    type Error = Stop;

    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Stop> {
        self.push(value)
    }

    fn end(self) -> Result<Described, Stop> {
        Ok(Items::end(self))
    }
}

impl ser::SerializeTuple for Items<'_> {
    type Ok = Described;
    type Error = Stop;

    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Stop> {
        self.push(value)
    }

    fn end(self) -> Result<Described, Stop> {
        Ok(Items::end(self))
    }
}

impl ser::SerializeTupleStruct for Items<'_> {
    type Ok = Described;
    type Error = Stop;

    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Stop> {
        self.push(value)
    }

    fn end(self) -> Result<Described, Stop> {
        Ok(Items::end(self))
    }
}

/// A struct's fields.
struct Fields<'t> {
    shaping: Shaping<'t>,
    fields: Vec<(&'static str, Described)>,
}

impl ser::SerializeStruct for Fields<'_> {
    type Ok = Described;
    type Error = Stop;

    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        name: &'static str,
        value: &T,
    ) -> Result<(), Stop> {
        self.fields.push((name, value.serialize(self.shaping)?));
        Ok(())
    }

    fn end(self) -> Result<Described, Stop> {
        Ok(Described {
            fields: self.fields,
            ..Described::of(Format::Unknown)
        })
    }
}
