use std::io::{Write, Result};
use {Class, Function, Type, TypeKind, typescript};
use heck::MixedCase;
use std::collections::BTreeMap;

fn get_hl_type_with_null(ty: &Type) -> String {
    let mut formatted = get_hl_type_without_null(ty);
    if ty.is_nullable {
        formatted.push_str(" | null");
    }
    formatted
}

fn get_hl_type_without_null(ty: &Type) -> String {
    if ty.is_custom {
        match ty.kind {
            TypeKind::Ref => format!("{}Ref", ty.name),
            TypeKind::RefMut => format!("{}RefMut", ty.name),
            TypeKind::Value => ty.name.clone(),
        }
    } else {
        match (ty.kind, ty.name.as_str()) {
            (TypeKind::Ref, "c_char") => "string",
            (TypeKind::Ref, _) | (TypeKind::RefMut, _) => "Buffer",
            (_, t) if !ty.is_custom => {
                match t {
                    "i8" => "number",
                    "i16" => "number",
                    "i32" => "number",
                    "i64" => "number",
                    "u8" => "number",
                    "u16" => "number",
                    "u32" => "number",
                    "u64" => "number",
                    "usize" => "number",
                    "f32" => "number",
                    "f64" => "number",
                    "bool" => "boolean",
                    "()" => "void",
                    "c_char" => "string",
                    "Json" => "any",
                    x => x,
                }
            }
            _ => unreachable!(),
        }.to_string()
    }
}

fn get_ll_type(ty: &Type) -> &str {
    match (ty.kind, ty.name.as_str()) {
        (TypeKind::Ref, "c_char") | (_, "Json") => "'CString'",
        (TypeKind::Ref, _) | (TypeKind::RefMut, _) => "'pointer'",
        (_, t) if !ty.is_custom => {
            match t {
                "i8" => "'int8'",
                "i16" => "'int16'",
                "i32" => "'int32'",
                "i64" => "'int64'",
                "u8" => "'uint8'",
                "u16" => "'uint16'",
                "u32" => "'uint32'",
                "u64" => "'uint64'",
                "usize" => "'size_t'",
                "f32" => "'float'",
                "f64" => "'double'",
                "bool" => "'bool'",
                "()" => "'void'",
                "c_char" => "'char'",
                x => x,
            }
        }
        _ => "'pointer'",
    }
}

fn write_fn<W: Write>(mut writer: W, function: &Function, type_script: bool) -> Result<()> {
    let is_static = function.is_static();
    let has_return_type = function.has_return_type();
    let return_type_with_null = get_hl_type_with_null(&function.output);
    let return_type_without_null = get_hl_type_without_null(&function.output);
    let method = function.method.to_mixed_case();
    let is_json = has_return_type && function.output.name == "Json";

    if !type_script {
        write!(
            writer,
            r#"
    /**"#
        )?;

        for &(ref name, ref ty) in function.inputs.iter().skip(if is_static { 0 } else { 1 }) {
            write!(
                writer,
                r#"
     * @param {{{}}} {}"#,
                get_hl_type_with_null(ty),
                name.to_mixed_case()
            )?;
        }

        if has_return_type {
            write!(
                writer,
                r#"
     * @return {{{}}}"#,
                return_type_with_null
            )?;
        }

        write!(
            writer,
            r#"
     */"#
        )?;
    }

    write!(
        writer,
        r#"
    {}{}("#,
        if is_static { "static " } else { "" },
        method
    )?;

    for (i, &(ref name, ref ty)) in function
        .inputs
        .iter()
        .skip(if is_static { 0 } else { 1 })
        .enumerate()
    {
        if i != 0 {
            write!(writer, ", ")?;
        }
        write!(writer, "{}", name.to_mixed_case())?;
        if type_script {
            write!(writer, ": {}", get_hl_type_with_null(ty))?;
        }
    }

    if type_script && has_return_type {
        write!(
            writer,
            r#"): {} {{
        "#,
            return_type_with_null
        )?;
    } else {
        write!(
            writer,
            r#") {{
        "#
        )?;
    }

    for &(ref name, ref typ) in function.inputs.iter() {
        if typ.is_custom {
            write!(
                writer,
                r#"if (ref.isNull({name}.ptr)) {{
            throw "{name} is disposed";
        }}
        "#,
                name = name.to_mixed_case()
            )?;
        }
    }

    if has_return_type {
        if function.output.is_custom {
            write!(writer, r#"var result = new {}("#, return_type_without_null)?;
        } else {
            write!(writer, "var result = ")?;
        }
    }

    write!(writer, r#"liveSplitCoreNative.{}("#, &function.name)?;

    for (i, &(ref name, ref typ)) in function.inputs.iter().enumerate() {
        if i != 0 {
            write!(writer, ", ")?;
        }
        write!(
            writer,
            "{}",
            if name == "this" {
                "this.ptr".to_string()
            } else if typ.name == "Json" {
                format!("JSON.stringify({})", name.to_mixed_case())
            } else if typ.is_custom {
                format!("{}.ptr", name.to_mixed_case())
            } else {
                name.to_mixed_case()
            }
        )?;
    }

    write!(writer, ")")?;

    if has_return_type && function.output.is_custom {
        write!(writer, r#")"#)?;
    }

    write!(writer, r#";"#)?;

    for &(ref name, ref typ) in function.inputs.iter() {
        if typ.is_custom && typ.kind == TypeKind::Value {
            write!(
                writer,
                r#"
        {}.ptr = ref.NULL;"#,
                name.to_mixed_case()
            )?;
        }
    }

    if has_return_type {
        if function.output.is_nullable && function.output.is_custom {
            write!(
                writer,
                r#"
        if (ref.isNull(result.ptr)) {{
            return null;
        }}"#
            )?;
        }
        if is_json {
            write!(
                writer,
                r#"
        return JSON.parse(result);"#
            )?;
        } else {
            write!(
                writer,
                r#"
        return result;"#
            )?;
        }
    }

    write!(
        writer,
        r#"
    }}"#
    )?;

    Ok(())
}

pub fn write<W: Write>(
    mut writer: W,
    classes: &BTreeMap<String, Class>,
    type_script: bool,
) -> Result<()> {
    if type_script {
        write!(
            writer,
            r#""use strict";
import ffi = require('ffi');
import fs = require('fs');
import ref = require('ref');

{}

var liveSplitCoreNative = ffi.Library('livesplit_core', {{"#,
            typescript::HEADER
        )?;
    } else {
        write!(
            writer,
            "{}",
            r#""use strict";
var ffi = require('ffi');
var fs = require('fs');
var ref = require('ref');

var liveSplitCoreNative = ffi.Library('livesplit_core', {"#
        )?;
    }

    for class in classes.values() {
        for function in class
            .static_fns
            .iter()
            .chain(class.own_fns.iter())
            .chain(class.shared_fns.iter())
            .chain(class.mut_fns.iter())
        {
            write!(
                writer,
                r#"
    '{}': [{}, ["#,
                function.name,
                get_ll_type(&function.output)
            )?;

            for (i, &(_, ref typ)) in function.inputs.iter().enumerate() {
                if i != 0 {
                    write!(writer, ", ")?;
                }
                write!(writer, "{}", get_ll_type(typ))?;
            }

            write!(writer, "]],")?;
        }
    }

    writeln!(
        writer,
        "{}",
        r#"
});"#
    )?;

    for (class_name, class) in classes {
        let class_name_ref = format!("{}Ref", class_name);
        let class_name_ref_mut = format!("{}RefMut", class_name);

        write!(
            writer,
            r#"
{export}class {class} {{"#,
            class = class_name_ref,
            export = if type_script { "export " } else { "" }
        )?;

        if type_script {
            write!(
                writer,
                r#"
    ptr: Buffer;"#
            )?;
        }

        for function in &class.shared_fns {
            write_fn(&mut writer, function, type_script)?;
        }

        if class_name == "SharedTimer" {
            if type_script {
                write!(
                    writer,
                    "{}",
                    r#"
    readWith(action: (timer: TimerRef) => void) {
        this.read().with(function (lock) {
            action(lock.timer());
        });
    }
    writeWith(action: (timer: TimerRefMut) => void) {
        this.write().with(function (lock) {
            action(lock.timer());
        });
    }"#
                )?;
            } else {
                write!(
                    writer,
                    "{}",
                    r#"
    /**
     * @param {function(TimerRef)} action
     */
    readWith(action) {
        this.read().with(function (lock) {
            action(lock.timer());
        });
    }
    /**
     * @param {function(TimerRefMut)} action
     */
    writeWith(action) {
        this.write().with(function (lock) {
            action(lock.timer());
        });
    }"#
                )?;
            }
        }

        if type_script {
            write!(
                writer,
                r#"
    constructor(ptr: Buffer) {{"#
            )?;
        } else {
            write!(
                writer,
                r#"
    /**
     * @param {{Buffer}} ptr
     */
    constructor(ptr) {{"#
            )?;
        }

        write!(
            writer,
            r#"
        this.ptr = ptr;
    }}
}}
{export}class {class} extends {base_class} {{"#,
            class = class_name_ref_mut,
            base_class = class_name_ref,
            export = if type_script {
                r#"
export "#.to_string()
            } else {
                format!(
                    r#"exports.{base_class} = {base_class};

"#,
                    base_class = class_name_ref
                )
            }
        )?;

        for function in &class.mut_fns {
            write_fn(&mut writer, function, type_script)?;
        }

        write!(
            writer,
            r#"
}}
{export}class {class} extends {base_class} {{"#,
            class = class_name,
            base_class = class_name_ref_mut,
            export = if type_script {
                r#"
export "#.to_string()
            } else {
                format!(
                    r#"exports.{base_class} = {base_class};

"#,
                    base_class = class_name_ref_mut
                )
            }
        )?;

        if type_script {
            write!(
                writer,
                r#"
    with(closure: (obj: {class}) => void) {{"#,
                class = class_name
            )?;
        } else {
            write!(
                writer,
                r#"
    /**
     * @param {{function({class})}} closure
     */
    with(closure) {{"#,
                class = class_name
            )?;
        }

        write!(
            writer,
            r#"
        try {{
            closure(this);
        }} finally {{
            this.dispose();
        }}
    }}
    dispose() {{
        if (!ref.isNull(this.ptr)) {{"#
        )?;

        if let Some(function) = class.own_fns.iter().find(|f| f.method == "drop") {
            write!(
                writer,
                r#"
            liveSplitCoreNative.{}(this.ptr);"#,
                function.name
            )?;
        }

        write!(
            writer,
            r#"
            this.ptr = ref.NULL;
        }}
    }}"#
        )?;

        for function in class.static_fns.iter().chain(class.own_fns.iter()) {
            if function.method != "drop" {
                write_fn(&mut writer, function, type_script)?;
            }
        }

        if class_name == "Run" {
            if type_script {
                write!(
                    writer,
                    "{}",
                    r#"
    static parseArray(data: Int8Array): Run {
        var buf = Buffer.from(data.buffer);
        if (data.byteLength !== data.buffer.byteLength) {
            buf = buf.slice(data.byteOffset, data.byteOffset + data.byteLength);
        }
        return Run.parse(buf, buf.byteLength);
    }
    static parseFile(file: any) {
        var data = fs.readFileSync(file);
        return Run.parse(data, data.byteLength);
    }
    static parseString(text: string): Run {
        let data = new Buffer(text);
        return Run.parse(data, data.byteLength);
    }"#
                )?;
            } else {
                write!(
                    writer,
                    "{}",
                    r#"
    /**
     * @param {Int8Array} data
     * @return {Run}
     */
    static parseArray(data) {
        var buf = Buffer.from(data.buffer);
        if (data.byteLength !== data.buffer.byteLength) {
            buf = buf.slice(data.byteOffset, data.byteOffset + data.byteLength);
        }
        return Run.parse(buf, buf.byteLength);
    }
    /**
     * @param {string | Buffer | number} file
     * @return {Run}
     */
    static parseFile(file: any) {
        var data = fs.readFileSync(file);
        return Run.parse(data, data.byteLength);
    }
    /**
     * @param {string} text
     * @return {Run}
     */
    static parseString(text: string): Run {
        let data = new Buffer(text);
        return Run.parse(data, data.byteLength);
    }"#
                )?;
            }
        }

        writeln!(
            writer,
            r#"
}}{export}"#,
            export = if type_script {
                "".to_string()
            } else {
                format!(
                    r#"
exports.{base_class} = {base_class};"#,
                    base_class = class_name
                )
            }
        )?;
    }

    Ok(())
}
