//! Array class

use crate::avm1::activation::Activation;
use crate::avm1::error::Error;
use crate::avm1::function::{Executable, FunctionObject};
use crate::avm1::property_decl::{define_properties_on, Declaration};
use crate::avm1::{AvmString, Object, ScriptObject, TObject, Value};
use bitflags::bitflags;
use gc_arena::MutationContext;
use std::cmp::Ordering;

bitflags! {
    /// Flags used by `Array.sort` and `Array.sortOn`.
    struct SortFlags: i32 {
        const CASE_INSENSITIVE     = 1 << 0;
        const DESCENDING           = 1 << 1;
        const UNIQUE_SORT          = 1 << 2;
        const RETURN_INDEXED_ARRAY = 1 << 3;
        const NUMERIC              = 1 << 4;
    }
}

// TODO: This won't work accurately in cases like NaN/undefined.
// We need to actually match Flash's sorting algorithm and not use Rust's Vec::sort.
/// Default ordering to return if comparison is invalid.
const DEFAULT_ORDERING: Ordering = Ordering::Equal;

/// Compare function used by `Array.sort` and `Array.sortOn`.
type CompareFn<'a, 'gc> =
    Box<dyn 'a + FnMut(&mut Activation<'_, 'gc, '_>, &Value<'gc>, &Value<'gc>) -> Ordering>;

const PROTO_DECLS: &[Declaration] = declare_properties! {
    "push" => method(push; DONT_ENUM);
    "unshift" => method(unshift; DONT_ENUM);
    "shift" => method(shift; DONT_ENUM);
    "pop" => method(pop; DONT_ENUM);
    "reverse" => method(reverse; DONT_ENUM);
    "join" => method(join; DONT_ENUM);
    "slice" => method(slice; DONT_ENUM);
    "splice" => method(splice; DONT_ENUM);
    "concat" => method(concat; DONT_ENUM);
    "toString" => method(to_string; DONT_ENUM);
    "sort" => method(sort; DONT_ENUM);
    "sortOn" => method(sort_on; DONT_ENUM);
};

const OBJECT_DECLS: &[Declaration] = declare_properties! {
    "CASEINSENSITIVE" => int(SortFlags::CASE_INSENSITIVE.bits(); DONT_ENUM | DONT_DELETE | READ_ONLY);
    "DESCENDING" => int(SortFlags::DESCENDING.bits(); DONT_ENUM | DONT_DELETE | READ_ONLY);
    "UNIQUESORT" => int(SortFlags::UNIQUE_SORT.bits(); DONT_ENUM | DONT_DELETE | READ_ONLY);
    "RETURNINDEXEDARRAY" => int(SortFlags::RETURN_INDEXED_ARRAY.bits(); DONT_ENUM | DONT_DELETE | READ_ONLY);
    "NUMERIC" => int(SortFlags::NUMERIC.bits(); DONT_ENUM | DONT_DELETE | READ_ONLY);
};

pub fn create_array_object<'gc>(
    gc_context: MutationContext<'gc, '_>,
    array_proto: Object<'gc>,
    fn_proto: Object<'gc>,
) -> Object<'gc> {
    let array = FunctionObject::constructor(
        gc_context,
        Executable::Native(constructor),
        Executable::Native(array_function),
        Some(fn_proto),
        array_proto,
    );
    let object = array.as_script_object().unwrap();

    // TODO: These were added in Flash Player 7, but are available even to SWFv6 and lower
    // when run in Flash Player 7. Make these conditional if we add a parameter to control
    // target Flash Player version.
    define_properties_on(OBJECT_DECLS, gc_context, object, fn_proto);
    array
}

/// Implements `Array` constructor
pub fn constructor<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let mut consumed = false;

    if args.len() == 1 {
        let arg = args.get(0).unwrap();
        if let Value::Number(length) = *arg {
            if length >= 0.0 {
                this.set_length(activation.context.gc_context, length as usize);
                consumed = true;
            } else if !length.is_nan() {
                this.set_length(activation.context.gc_context, 0);
                consumed = true;
            }
        }
    }

    if !consumed {
        for (i, arg) in args.iter().enumerate() {
            this.set_array_element(i, arg.to_owned(), activation.context.gc_context);
        }
    }

    Ok(this.into())
}

/// Implements `Array` function
pub fn array_function<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    _this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let mut consumed = false;

    let prototype = activation.context.avm1.prototypes.array;
    let array_obj = prototype.create_bare_object(activation, prototype)?;

    if args.len() == 1 {
        let arg = args.get(0).unwrap();
        if let Value::Number(length) = *arg {
            if length >= 0.0 {
                array_obj.set_length(activation.context.gc_context, length as usize);
                consumed = true;
            } else if !length.is_nan() {
                array_obj.set_length(activation.context.gc_context, 0);
                consumed = true;
            }
        }
    }

    if !consumed {
        for (i, arg) in args.iter().enumerate() {
            array_obj.set_array_element(i, arg.to_owned(), activation.context.gc_context);
        }
    }

    Ok(array_obj.into())
}

pub fn push<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let old_length = this.length();
    let new_length = old_length + args.len();
    this.set_length(activation.context.gc_context, new_length);

    for i in 0..args.len() {
        this.set_array_element(
            old_length + i,
            args.get(i).unwrap().to_owned(),
            activation.context.gc_context,
        );
    }

    Ok((new_length as f64).into())
}

pub fn unshift<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let old_length = this.length();
    let new_length = old_length + args.len();
    let offset = args.len();

    if old_length > 0 {
        // Move all elements up by [offset], in reverse order.
        for i in (offset..new_length).rev() {
            this.set_array_element(
                i,
                this.array_element(i - offset),
                activation.context.gc_context,
            );
        }
    }

    for i in 0..args.len() {
        // Put the new elements at the start of the array.
        this.set_array_element(
            i,
            args.get(i).unwrap().to_owned(),
            activation.context.gc_context,
        );
    }

    this.set_length(activation.context.gc_context, new_length);

    Ok((new_length as f64).into())
}

pub fn shift<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let old_length = this.length();
    if old_length == 0 {
        return Ok(Value::Undefined);
    }

    let new_length = old_length - 1;

    let removed = this.array_element(0);

    for i in 0..new_length {
        this.set_array_element(i, this.array_element(i + 1), activation.context.gc_context);
    }

    this.delete_array_element(new_length, activation.context.gc_context);
    this.delete(activation, &new_length.to_string());

    this.set_length(activation.context.gc_context, new_length);

    Ok(removed)
}

pub fn pop<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let old_length = this.length();
    if old_length == 0 {
        return Ok(Value::Undefined);
    }

    let new_length = old_length - 1;

    let removed = this.array_element(new_length);
    this.delete_array_element(new_length, activation.context.gc_context);
    this.delete(activation, &new_length.to_string());

    this.set_length(activation.context.gc_context, new_length);

    Ok(removed)
}

pub fn reverse<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let length = this.length();
    let mut values = this.array().to_vec();

    for i in 0..length {
        this.set_array_element(i, values.pop().unwrap(), activation.context.gc_context);
    }

    // Some docs incorrectly say reverse returns Void.
    Ok(this.into())
}

pub fn join<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let separator = args
        .get(0)
        .and_then(|v| v.coerce_to_string(activation).ok())
        .unwrap_or_else(|| ",".into());
    let values: Vec<Value<'gc>> = this.array();

    Ok(AvmString::new(
        activation.context.gc_context,
        values
            .iter()
            .map(|v| {
                v.coerce_to_string(activation)
                    .unwrap_or_else(|_| "undefined".into())
                    .to_string()
            })
            .collect::<Vec<String>>()
            .join(&separator),
    )
    .into())
}

/// Handles an index parameter that may be positive (starting from beginning) or negaitve (starting from end).
/// The returned index will be positive and clamped from [0, length].
fn make_index_absolute(index: i32, length: usize) -> usize {
    if index < 0 {
        let offset = index as isize;
        length.saturating_sub((-offset) as usize)
    } else {
        (index as usize).min(length)
    }
}

pub fn slice<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let start = args
        .get(0)
        .and_then(|v| v.coerce_to_f64(activation).ok())
        .map(|v| make_index_absolute(v as i32, this.length()))
        .unwrap_or(0);
    let end = args
        .get(1)
        .and_then(|v| v.coerce_to_f64(activation).ok())
        .map(|v| make_index_absolute(v as i32, this.length()))
        .unwrap_or_else(|| this.length());

    let array = ScriptObject::array(
        activation.context.gc_context,
        Some(activation.context.avm1.prototypes.array),
    );

    if start < end {
        let length = end - start;
        array.set_length(activation.context.gc_context, length);

        for i in 0..length {
            array.set_array_element(
                i,
                this.array_element(start + i),
                activation.context.gc_context,
            );
        }
    }

    Ok(array.into())
}

pub fn splice<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if args.is_empty() {
        return Ok(Value::Undefined);
    }

    let old_length = this.length();
    let start = args
        .get(0)
        .and_then(|v| v.coerce_to_f64(activation).ok())
        .map(|v| make_index_absolute(v as i32, old_length))
        .unwrap_or(0);
    let count = args
        .get(1)
        .and_then(|v| v.coerce_to_f64(activation).ok())
        .map(|v| v as i32)
        .unwrap_or(old_length as i32);
    if count < 0 {
        return Ok(Value::Undefined);
    }

    let removed = ScriptObject::array(
        activation.context.gc_context,
        Some(activation.context.avm1.prototypes.array),
    );
    let to_remove = count.min(old_length as i32 - start as i32).max(0) as usize;
    let to_add = if args.len() > 2 { &args[2..] } else { &[] };
    let offset = to_remove as i32 - to_add.len() as i32;
    let new_length = old_length + to_add.len() - to_remove;
    for i in start..start + to_remove {
        removed.set_array_element(
            i - start,
            this.array_element(i),
            activation.context.gc_context,
        );
    }
    removed.set_length(activation.context.gc_context, to_remove);

    if offset < 0 {
        for i in (start + to_add.len()..new_length).rev() {
            this.set_array_element(
                i,
                this.array_element((i as i32 + offset) as usize),
                activation.context.gc_context,
            );
        }
    } else {
        for i in start + to_add.len()..new_length {
            this.set_array_element(
                i,
                this.array_element((i as i32 + offset) as usize),
                activation.context.gc_context,
            );
        }
    }

    for i in 0..to_add.len() {
        this.set_array_element(
            start + i,
            to_add.get(i).unwrap().to_owned(),
            activation.context.gc_context,
        );
    }

    for i in new_length..old_length {
        this.delete_array_element(i, activation.context.gc_context);
        this.delete(activation, &i.to_string());
    }

    this.set_length(activation.context.gc_context, new_length);

    Ok(removed.into())
}

pub fn concat<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let array = ScriptObject::array(
        activation.context.gc_context,
        Some(activation.context.avm1.prototypes.array),
    );
    let mut length = 0;

    for i in 0..this.length() {
        let old = this
            .get(&i.to_string(), activation)
            .unwrap_or(Value::Undefined);
        array.set_array_element(length, old, activation.context.gc_context);
        length += 1;
    }

    for arg in args {
        let mut added = false;

        if let Value::Object(object) = arg {
            let object = *object;
            if activation
                .context
                .avm1
                .prototypes
                .array
                .is_prototype_of(object)
            {
                added = true;
                for i in 0..object.length() {
                    let old = object
                        .get(&i.to_string(), activation)
                        .unwrap_or(Value::Undefined);
                    array.set_array_element(length, old, activation.context.gc_context);
                    length += 1;
                }
            }
        }

        if !added {
            array.set_array_element(length, *arg, activation.context.gc_context);
            length += 1;
        }
    }

    Ok(array.into())
}

pub fn to_string<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    join(activation, this, &[])
}

fn sort<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    // Overloads:
    // 1) a.sort(flags: Number = 0): Sorts with the given flags.
    // 2) a.sort(compare_fn: Object, flags: Number = 0): Sorts using the given compare function and flags.
    use crate::ecma_conversions::f64_to_wrapping_i32;
    let (compare_fn, flags) = match args {
        [Value::Number(_), Value::Number(n), ..] => (None, f64_to_wrapping_i32(*n)),
        [Value::Number(n), ..] => (None, f64_to_wrapping_i32(*n)),
        [compare_fn @ Value::Object(_), Value::Number(n), ..] => {
            (Some(compare_fn), f64_to_wrapping_i32(*n))
        }
        [compare_fn @ Value::Object(_), ..] => (Some(compare_fn), 0),
        [] => (None, 0),
        _ => return Ok(Value::Undefined),
    };
    let flags = SortFlags::from_bits_truncate(flags);

    let string_compare_fn = if flags.contains(SortFlags::CASE_INSENSITIVE) {
        sort_compare_string_ignore_case
    } else {
        sort_compare_string
    };

    let compare_fn: CompareFn<'_, 'gc> = if let Some(f) = compare_fn {
        let this = Value::Undefined.coerce_to_object(activation);
        // this is undefined in the compare function
        Box::new(move |activation, a: &Value<'gc>, b: &Value<'gc>| {
            sort_compare_custom(activation, this, a, b, f)
        })
    } else if flags.contains(SortFlags::NUMERIC) {
        Box::new(sort_compare_numeric(
            flags.contains(SortFlags::CASE_INSENSITIVE),
        ))
    } else {
        Box::new(string_compare_fn)
    };

    sort_with_function(activation, this, compare_fn, flags)
}

fn sort_on<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    // a.sortOn(field_name, flags: Number = 0): Sorts with the given flags.
    // a.sortOn(field_names: Array, flags: Number = 0): Sorts with fields in order of precedence with the given flags.
    // a.sortOn(field_names: Array, flags: Array): Sorts with fields in order of precedence with the given flags respectively.
    let fields = match args.get(0) {
        Some(Value::Object(array)) => {
            // Array of field names.
            let mut field_names = vec![];
            for name in array.array() {
                field_names.push(name.coerce_to_string(activation)?.to_string());
            }
            field_names
        }
        Some(field_name) => {
            // Single field.
            vec![field_name.coerce_to_string(activation)?.to_string()]
        }
        None => return Ok(Value::Undefined),
    };

    // Bail out if we don't have any fields.
    if fields.is_empty() {
        return Ok(this.into());
    }

    let flags = match args.get(1) {
        Some(Value::Object(array)) => {
            // Array of field names.
            if array.length() == fields.len() {
                let mut flags = vec![];
                for flag in array.array() {
                    flags.push(SortFlags::from_bits_truncate(
                        flag.coerce_to_i32(activation)?,
                    ));
                }
                flags
            } else {
                // If the lengths of the flags and fields array do not match, the flags array is ignored.
                std::iter::repeat(SortFlags::empty())
                    .take(fields.len())
                    .collect()
            }
        }
        Some(flags) => {
            // Single field.
            let flags = SortFlags::from_bits_truncate(flags.coerce_to_i32(activation)?);
            std::iter::repeat(flags).take(fields.len()).collect()
        }
        None => std::iter::repeat(SortFlags::empty())
            .take(fields.len())
            .collect(),
    };

    // CASEINSENSITIVE, UNIQUESORT, and RETURNINDEXEDARRAY are taken from the first set of flags in the array.
    let main_flags = flags[0];

    // Generate a list of compare functions to use for each field in the array.
    let field_compare_fns: Vec<CompareFn<'_, 'gc>> = flags
        .into_iter()
        .map(|flags| {
            let string_compare_fn = if flags.contains(SortFlags::CASE_INSENSITIVE) {
                sort_compare_string_ignore_case
            } else {
                sort_compare_string
            };

            if flags.contains(SortFlags::NUMERIC) {
                Box::new(sort_compare_numeric(
                    flags.contains(SortFlags::CASE_INSENSITIVE),
                ))
            } else {
                Box::new(string_compare_fn) as CompareFn<'_, 'gc>
            }
        })
        .collect();

    let compare_fn = sort_compare_fields(fields, field_compare_fns);

    sort_with_function(activation, this, compare_fn, main_flags)
}

fn sort_with_function<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    mut compare_fn: impl FnMut(&mut Activation<'_, 'gc, '_>, &Value<'gc>, &Value<'gc>) -> Ordering,
    flags: SortFlags,
) -> Result<Value<'gc>, Error<'gc>> {
    let length = this.length();
    let mut values: Vec<(usize, Value<'gc>)> = this.array().into_iter().enumerate().collect();
    let array_proto = activation.context.avm1.prototypes.array;

    let mut is_unique = true;
    values.sort_unstable_by(|a, b| {
        let mut ret = compare_fn(activation, &a.1, &b.1);
        if flags.contains(SortFlags::DESCENDING) {
            ret = ret.reverse();
        }
        if ret == Ordering::Equal {
            is_unique = false;
        }
        ret
    });

    if flags.contains(SortFlags::UNIQUE_SORT) && !is_unique {
        // Check for uniqueness. Return 0 if there is a duplicated value.
        return Ok(0.into());
    }

    if flags.contains(SortFlags::RETURN_INDEXED_ARRAY) {
        // Array.RETURNINDEXEDARRAY returns an array containing the sorted indices, and does not modify
        // the original array.
        let array = ScriptObject::array(activation.context.gc_context, Some(array_proto));
        array.set_length(activation.context.gc_context, length);
        for (i, value) in values.into_iter().enumerate() {
            array.set_array_element(
                i,
                Value::Number(value.0 as f64),
                activation.context.gc_context,
            );
        }
        Ok(array.into())
    } else {
        // Standard sort modifies the original array, and returns it.
        // AS2 reference incorrectly states this returns nothing, but it returns the original array, sorted.
        for (i, value) in values.into_iter().enumerate() {
            this.set_array_element(i, value.1, activation.context.gc_context);
        }
        Ok(this.into())
    }
}

pub fn create_proto<'gc>(
    gc_context: MutationContext<'gc, '_>,
    proto: Object<'gc>,
    fn_proto: Object<'gc>,
) -> Object<'gc> {
    let array = ScriptObject::array(gc_context, Some(proto));
    let object = array.as_script_object().unwrap();
    define_properties_on(PROTO_DECLS, gc_context, object, fn_proto);
    array.into()
}

fn sort_compare_string<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    a: &Value<'gc>,
    b: &Value<'gc>,
) -> Ordering {
    let a_str = a.coerce_to_string(activation);
    let b_str = b.coerce_to_string(activation);
    // TODO: Handle errors.
    if let (Ok(a_str), Ok(b_str)) = (a_str, b_str) {
        a_str.cmp(&b_str)
    } else {
        DEFAULT_ORDERING
    }
}

fn sort_compare_string_ignore_case<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    a: &Value<'gc>,
    b: &Value<'gc>,
) -> Ordering {
    let a_str = a.coerce_to_string(activation);
    let b_str = b.coerce_to_string(activation);
    // TODO: Handle errors.
    if let (Ok(a_str), Ok(b_str)) = (a_str, b_str) {
        crate::string_utils::swf_string_cmp_ignore_case(&a_str, &b_str)
    } else {
        DEFAULT_ORDERING
    }
}

fn sort_compare_numeric<'gc>(
    case_insensitive: bool,
) -> impl FnMut(&mut Activation<'_, 'gc, '_>, &Value<'gc>, &Value<'gc>) -> Ordering {
    move |activation, a, b| {
        if let (Value::Number(a), Value::Number(b)) = (a, b) {
            a.partial_cmp(b).unwrap_or(DEFAULT_ORDERING)
        } else if case_insensitive {
            sort_compare_string_ignore_case(activation, a, b)
        } else {
            sort_compare_string(activation, a, b)
        }
    }
}

fn sort_compare_fields<'a, 'gc: 'a>(
    field_names: Vec<String>,
    mut compare_fns: Vec<CompareFn<'a, 'gc>>,
) -> impl 'a + FnMut(&mut Activation<'_, 'gc, '_>, &Value<'gc>, &Value<'gc>) -> Ordering {
    move |activation, a, b| {
        for (field_name, compare_fn) in field_names.iter().zip(compare_fns.iter_mut()) {
            let a_object = a.coerce_to_object(activation);
            let b_object = b.coerce_to_object(activation);
            let a_prop = a_object.get(field_name, activation).unwrap();
            let b_prop = b_object.get(field_name, activation).unwrap();

            let result = compare_fn(activation, &a_prop, &b_prop);
            if result != Ordering::Equal {
                return result;
            }
        }
        // Got through all fields; must be equal.
        Ordering::Equal
    }
}

// Returning an impl Trait here doesn't work yet because of https://github.com/rust-lang/rust/issues/65805 (?)
fn sort_compare_custom<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    a: &Value<'gc>,
    b: &Value<'gc>,
    compare_fn: &Value<'gc>,
) -> Ordering {
    // TODO: Handle errors.
    let args = [*a, *b];
    let ret = compare_fn
        .call("[Compare]", activation, this, None, &args)
        .unwrap_or(Value::Undefined);
    match ret {
        Value::Number(n) if n > 0.0 => Ordering::Greater,
        Value::Number(n) if n < 0.0 => Ordering::Less,
        Value::Number(n) if n == 0.0 => Ordering::Equal,
        _ => DEFAULT_ORDERING,
    }
}
