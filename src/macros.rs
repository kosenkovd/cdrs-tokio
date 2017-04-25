/// instead of writing functions which resemble
/// ```
/// pub fn query<'a> (&'a mut self,query: String) -> &'a mut Self{
///     self.query = Some(query);
///            self
/// }
/// ```
/// and repeating it for all the attributes; it is extracted out as a macro so that code
/// is more concise see
/// @https://doc.rust-lang.org/book/method-syntax.html
///
///
///
macro_rules! builder_opt_field {
    ($field:ident, $field_type:ty) => {
        pub fn $field(mut self,
                          $field: $field_type) -> Self {
            self.$field = Some($field);
            self
        }
    };
}



macro_rules! list_as_rust {
    ($($into_type:tt)*) => (
        impl AsRustType<Vec<$($into_type)*>> for List {
            fn as_rust_type(&self) -> Result<Option<Vec<$($into_type)*>>> {
                match self.metadata.value {
                    Some(ColTypeOptionValue::CList(ref type_option)) |
                    Some(ColTypeOptionValue::CSet(ref type_option)) => {
                        let type_option_ref = type_option.as_ref();
                        let convert = self
                            .map(|bytes| {
                                as_rust_type!(type_option_ref, bytes, $($into_type)*)
                                    .unwrap()
                                    // item in a list supposed to be a non-null value.
                                    // TODO: check if it's true
                                    .unwrap()
                            });

                        Ok(Some(convert))
                    },
                    _ => Err(Error::General(format!("Invalid conversion. \
                            Cannot convert {:?} into List (valid types: List, Set).",
                            self.metadata.value)))
                }
            }
        }
    );
}



macro_rules! map_as_rust {
    ($(K $key_type:tt)*, $(V $val_type:tt)*) => (
        impl AsRustType<HashMap<$($key_type)*, $($val_type)*>> for Map {
            /// Converts `Map` into `HashMap` for blob values.
            fn as_rust_type(&self) -> Result<Option<HashMap<$($key_type)*, $($val_type)*>>> {
                match self.metadata.value {
                    Some(ColTypeOptionValue::CMap((ref key_type_option, ref val_type_option))) => {
                        let mut map = HashMap::with_capacity(self.data.len());

                        for &(ref key, ref val) in self.data.iter() {
                            let key_type_option = key_type_option.as_ref();
                            let val_type_option = val_type_option.as_ref();
                            // key is supposed to be neither null nor non-set value
                            let key = as_rust_type!(key_type_option, key, $($key_type)*)?.unwrap();
                            let val = as_rust_type!(val_type_option, val, $($val_type)*)?;
                            if val.is_some() {
                                map.insert(key, val.unwrap());
                            }
                        }

                        Ok(Some(map))
                    }
                    _ => unreachable!()
                }
            }
        }
    );
}




macro_rules! into_rust_by_name {
    (Row, $($into_type:tt)*) => (
        impl IntoRustByName<$($into_type)*> for Row {
            fn get_by_name(&self, name: &str) -> Result<Option<$($into_type)*>> {
                self.get_col_spec_by_name(name)
                    .ok_or(column_is_empty_err())
                    .and_then(|(col_spec, cbytes)| {
                        // if cbytes.is_empty() {
                        //     return Err(column_is_empty_err());
                        // }

                        let ref col_type = col_spec.col_type;
                        as_rust_type!(col_type, cbytes, $($into_type)*)
                    })
            }
        }
    );

    (UDT, $($into_type:tt)*) => (
        impl IntoRustByName<$($into_type)*> for UDT {
            fn get_by_name(&self, name: &str) -> Result<Option<$($into_type)*>> {
                self.data.get(name)
                .ok_or(column_is_empty_err())
                .and_then(|v| {
                    let &(ref col_type, ref bytes) = v;

                    // if bytes.as_plain().is_empty() {
                    //     return Err(column_is_empty_err());
                    // }

                    let converted = as_rust_type!(col_type, bytes, $($into_type)*);
                    converted.map_err(|err| err.into())
                })
            }
        }
    );
}

macro_rules! as_res_opt {
    ($data_value:ident, $deserialize:expr) => (
        match $data_value.as_plain() {
            Some(ref bytes) => {
                ($deserialize)(bytes)
                    .map(|v| Some(v))
                    .map_err(Into::into)
            },
            None => Ok(None)
        }
    )
}

/// Decodes any Cassandra data type into the corresponding Rust type,
/// given the column type as `ColTypeOption` and the value as `CBytes`
/// plus the matching Rust type.
macro_rules! as_rust_type {
    ($data_type_option:ident, $data_value:ident, Vec<u8>) => (
        match $data_type_option.id {
            ColType::Blob => {
                as_res_opt!($data_value, decode_blob)
                // decode_blob($data_value.as_plain())
                //     .map_err(Into::into)
            }
            _ => Err(Error::General(format!("Invalid conversion. \
                    Cannot convert {:?} into Vec<u8> (valid types: Blob).",
                    $data_type_option.id)))
        }
    );
    ($data_type_option:ident, $data_value:ident, String) => (
        match $data_type_option.id {
            ColType::Custom => {
                as_res_opt!($data_value, decode_custom)
                // decode_custom($data_value.as_slice())
                //     .map_err(Into::into)
            }
            ColType::Ascii => {
                as_res_opt!($data_value, decode_ascii)
                // decode_ascii($data_value.as_slice())
                //     .map_err(Into::into)
            }
            ColType::Varchar => {
                as_res_opt!($data_value, decode_varchar)
                // decode_varchar($data_value.as_slice())
                //     .map_err(Into::into)
            }
            // TODO: clarify when to use decode_text.
            // it's not mentioned in
            // https://github.com/apache/cassandra/blob/trunk/doc/native_protocol_v4.spec#L582
            // ColType::XXX => decode_text($data_value)?
            _ => Err(Error::General(format!("Invalid conversion. \
                    Cannot convert {:?} into String (valid types: Custom, Ascii, Varchar).",
                    $data_type_option.id)))
        }
    );
    ($data_type_option:ident, $data_value:ident, bool) => (
        match $data_type_option.id {
            ColType::Boolean => {
                as_res_opt!($data_value, decode_boolean)
                // decode_boolean($data_value.as_slice())
                //     .map_err(Into::into)
            }
            _ => Err(Error::General(format!("Invalid conversion. \
                    Cannot convert {:?} into bool (valid types: Boolean).",
                    $data_type_option.id)))
        }
    );
    ($data_type_option:ident, $data_value:ident, i64) => (
        match $data_type_option.id {
            ColType::Bigint => {
                as_res_opt!($data_value, decode_bigint)
                // decode_bigint($data_value.as_slice())
                //     .map_err(Into::into)
            }
            ColType::Timestamp => {
                as_res_opt!($data_value, decode_timestamp)
                // decode_timestamp($data_value.as_slice())
                //     .map_err(Into::into)
            }
            ColType::Time => {
                as_res_opt!($data_value, decode_time)
                // decode_time($data_value.as_slice())
                //     .map_err(Into::into)
            }
            ColType::Varint => {
                as_res_opt!($data_value, decode_varint)
                // decode_varint($data_value.as_slice())
                //     .map_err(Into::into)
            }
            _ => Err(Error::General(format!("Invalid conversion. \
                    Cannot convert {:?} into i64 (valid types: Bigint, Timestamp, Time, Variant).",
                    $data_type_option.id)))
        }
    );
    ($data_type_option:ident, $data_value:ident, i32) => (
        match $data_type_option.id {
            ColType::Int => {
                as_res_opt!($data_value, decode_int)
                // decode_int($data_value.as_slice())
                //     .map_err(Into::into)
            }
            ColType::Date => {
                as_res_opt!($data_value, decode_date)
                // decode_date($data_value.as_slice())
                //     .map_err(Into::into)
            }
            _ => Err(Error::General(format!("Invalid conversion. \
                    Cannot convert {:?} into i32 (valid types: Int, Date).",
                    $data_type_option.id)))
        }
    );
    ($data_type_option:ident, $data_value:ident, i16) => (
        match $data_type_option.id {
            ColType::Smallint => {
                as_res_opt!($data_value, decode_smallint)
                // decode_smallint($data_value.as_slice())
                //     .map_err(Into::into)
            }
            _ => Err(Error::General(format!("Invalid conversion. \
                    Cannot convert {:?} into i16 (valid types: Smallint).",
                    $data_type_option.id)))
        }
    );
    ($data_type_option:ident, $data_value:ident, i8) => (
        match $data_type_option.id {
            ColType::Tinyint => {
                as_res_opt!($data_value, decode_tinyint)
                // decode_tinyint($data_value.as_slice())
                //     .map_err(Into::into)
            }
            _ => Err(Error::General(format!("Invalid conversion. \
                    Cannot convert {:?} into i8 (valid types: Tinyint).",
                    $data_type_option.id)))
        }
    );
    ($data_type_option:ident, $data_value:ident, f64) => (
        match $data_type_option.id {
            ColType::Double => {
                as_res_opt!($data_value, decode_double)
                // decode_double($data_value.as_slice())
                //     .map_err(Into::into)
            }
            _ => Err(Error::General(format!("Invalid conversion. \
                    Cannot convert {:?} into f64 (valid types: Double).",
                    $data_type_option.id)))
        }
    );
    ($data_type_option:ident, $data_value:ident, f32) => (
        match $data_type_option.id {
            ColType::Decimal => {
                as_res_opt!($data_value, decode_decimal)
                // decode_decimal($data_value.as_slice())
                //     .map_err(Into::into)
            }
            ColType::Float => {
                as_res_opt!($data_value, decode_float)
                // decode_float($data_value.as_slice())
                //     .map_err(Into::into)
            }
            _ => Err(Error::General(format!("Invalid conversion. \
                    Cannot convert {:?} into f32 (valid types: Decimal, Float).",
                    $data_type_option.id)))
        }
    );
    ($data_type_option:ident, $data_value:ident, IpAddr) => (
        match $data_type_option.id {
            ColType::Inet => {
                as_res_opt!($data_value, decode_inet)
                // decode_inet($data_value.as_slice())
                //     .map_err(Into::into)
            }
            _ => Err(Error::General(format!("Invalid conversion. \
                    Cannot convert {:?} into IpAddr (valid types: Inet).",
                    $data_type_option.id)))
        }
    );
    ($data_type_option:ident, $data_value:ident, Uuid) => (
        match $data_type_option.id {
            ColType::Uuid |
            ColType::Timeuuid => {
                as_res_opt!($data_value, decode_timeuuid)
                // decode_timeuuid($data_value.as_slice())
                //     .map_err(Into::into)
            }
            _ => Err(Error::General(format!("Invalid conversion. \
                    Cannot convert {:?} into Uuid (valid types: Uuid, Timeuuid).",
                    $data_type_option.id)))
        }
    );
    ($data_type_option:ident, $data_value:ident, List) => (
        match $data_type_option.id {
            ColType::List |
            ColType::Set => {

                // TODO: try this
                // match $data_value.as_slice() {
                //     Some(ref bytes) => decode_list(bytes)
                //         .map(|data| Some(List::new(data, $data_type_option.clone()))),
                //     None => Ok(None)
                // }

                decode_list($data_value.as_slice().unwrap())
                    .map(|data| Some(List::new(data, $data_type_option.clone())))
                    .map_err(Into::into)
            }
            _ => Err(Error::General(format!("Invalid conversion. \
                    Cannot convert {:?} into List (valid types: List, Set).",
                    $data_type_option.id)))
        }
    );
    ($data_type_option:ident, $data_value:ident, Map) => (
        match $data_type_option.id {
            ColType::Map => {
                // XXX unwrap Option
                decode_map($data_value.as_slice().unwrap())
                    .map(|data| Some(Map::new(data, $data_type_option.clone())))
                    .map_err(Into::into)
            }
            _ => Err(Error::General(format!("Invalid conversion. \
                    Cannot convert {:?} into Map (valid types: Map).",
                    $data_type_option.id)))
        }
    );
    ($data_type_option:ident, $data_value:ident, UDT) => (
        match *$data_type_option {
            ColTypeOption {
                id: ColType::Udt,
                value: Some(ColTypeOptionValue::UdtType(ref list_type_option))
            } => {
                // XXX: unwrap Option
                decode_udt($data_value.as_slice().unwrap(), list_type_option.descriptions.len())
                    .map(|data| Some(UDT::new(data, list_type_option)))
                    .map_err(Into::into)
            }
            _ => Err(Error::General(format!("Invalid conversion. \
                    Cannot convert {:?} into UDT (valid types: UDT).",
                    $data_type_option.id)))
        }
    );
    ($data_type_option:ident, $data_value:ident, Timespec) => (
        match $data_type_option.id {
            ColType::Timestamp => {
                decode_timestamp($data_value.as_slice().unwrap())
                    .map(|ts| Some(Timespec::new(ts / 1_000, (ts % 1_000 * 1_000_000) as i32)))
                    .map_err(Into::into)
            }
            _ => Err(Error::General(format!("Invalid conversion. \
                    Cannot convert {:?} into Timespec (valid types: Timestamp).",
                    $data_type_option.id)))
        }
    );
}
