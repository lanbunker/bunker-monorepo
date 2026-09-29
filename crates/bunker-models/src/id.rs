/// Declares a uuid identifier. Each one is its own type, so an identifier of the
/// wrong kind is a compile error and not an empty lookup.
macro_rules! uuid_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[nutype::nutype(derive(
            Debug,
            Clone,
            Copy,
            PartialEq,
            Eq,
            Hash,
            Display,
            Serialize,
            Deserialize
        ))]
        pub struct $name(uuid::Uuid);

        impl $name {
            pub fn generate() -> Self {
                Self::new(uuid::Uuid::new_v4())
            }
        }
    };
}

pub(crate) use uuid_id;
