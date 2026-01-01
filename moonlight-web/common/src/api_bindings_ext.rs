use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

#[macro_export]
macro_rules! ts_consts {
    ($struct_vis: vis $struct: ident $(( $test_name: ident : $path: expr ))? $( as $record_ty: ident)? : $($vis: vis const $name: ident : $ty: ident = $const: expr;)*) => {
        $struct_vis struct $struct;

        #[allow(unused)]
        impl $struct {
            $(
                $vis const $name: $ty = $const;
            )*
        }

        #[allow(clippy::unwrap_used)]
        impl TS for $struct {
            type WithoutGenerics = Self;

            type OptionInnerType = Self;

            fn decl() -> String {
                use std::fmt::Write as _;

                let mut decl = String::new();

                write!(&mut decl, "const {}", stringify!($struct)).unwrap();
                write!(&mut decl, " = {}", Self::inline()).unwrap();
                write!(&mut decl, ";").unwrap();

                decl
            }

            fn decl_concrete() -> String {
                Self::decl()
            }

            fn name() -> String {
                stringify!($struct).to_string()
            }

            fn inline() -> String {
                use std::fmt::Write as _;

                let mut inline = String::new();

                write!(&mut inline, "{{ ").unwrap();
                $(
                    let value: $ty = $struct::$name;

                    write!(&mut inline, "{}: {}, ", stringify!($name), value).unwrap();
                )*
                write!(&mut inline, "}}").unwrap();

                inline
            }

            fn inline_flattened() -> String {
                format!("({})", Self::inline())
            }

            $(
            fn output_path() -> Option<std::path::PathBuf> {
                Some(std::path::PathBuf::from({
                    let dir_or_file = format!("{}", $path);
                    if dir_or_file.ends_with('/') {
                        format!("{dir_or_file}{}.ts", stringify!($struct))
                    } else {
                        format!("{dir_or_file}")
                    }
                }))
            }
            )?
        }

        $(
        #[cfg(test)]
        #[test]
        fn $test_name() {
            <$struct as ::ts_rs::TS>::export_all().expect("could not export consts");
        }
        )?
    };

}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(from = "Value", into = "Value")]
pub struct TsAny(Value);

impl From<Value> for TsAny {
    fn from(value: Value) -> Self {
        Self(value)
    }
}
impl From<TsAny> for Value {
    fn from(value: TsAny) -> Self {
        value.0
    }
}

impl TS for TsAny {
    type WithoutGenerics = Self;
    type OptionInnerType = Self;

    fn decl() -> String {
        "any".to_string()
    }

    fn decl_concrete() -> String {
        Self::decl()
    }

    fn name() -> String {
        "any".to_string()
    }

    fn inline() -> String {
        "any".to_string()
    }

    fn inline_flattened() -> String {
        format!("({})", Self::inline())
    }
}
