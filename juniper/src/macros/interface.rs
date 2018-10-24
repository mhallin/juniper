/**
Expose GraphQL interfaces

Mapping interfaces to GraphQL can be tricky: there is no direct counterpart to
GraphQL interfaces in Rust, and downcasting is not possible in the general case.
Many other GraphQL implementations in other languages use instance checks and
either dynamic typing or forced downcasts to support these features.

A GraphQL interface defines fields that the implementing types also need to
implement. A GraphQL interface also needs to be able to determine the concrete
type name as well as downcast the general type to the actual concrete type.

## Syntax

See the documentation for [`graphql_object!`][1] on the general item and type
syntax. `graphql_interface!` requires an additional `instance_resolvers` item,
and does _not_ support the `interfaces` item.

`instance_resolvers` is a match like structure used to resolve the concrete
instance type of the interface. It starts with a context argument and continues
with a number of match arms; on the left side is the indicated type, and on the
right an expression that resolve into `Option<T>` of the type indicated:

```rust,ignore
instance_resolvers: |&context| {
    &Human => context.get_human(self.id()), // returns Option<&Human>
    &Droid => context.get_droid(self.id()), // returns Option<&Droid>
},
```

This is used for both the `__typename` field and when resolving a specialized
fragment, e.g. `...on Human`. For `__typename`, the resolvers will be executed
in order - the first one returning `Some` will be the determined type name. When
resolving fragment type conditions, only the corresponding match arm will be
executed.

## Example

A simplified extract from the StarWars schema example shows how to use the
shared context to implement downcasts.

```rust
# #[macro_use] extern crate juniper;
# use std::collections::HashMap;
struct Human { id: String }
struct Droid { id: String }
struct Database {
    humans: HashMap<String, Human>,
    droids: HashMap<String, Droid>,
}

trait Character {
    fn id(&self) -> &str;
}

impl Character for Human {
    fn id(&self) -> &str { &self.id }
}

impl Character for Droid {
    fn id(&self) -> &str { &self.id }
}

graphql_object!(Human: Database as "Human" |&self| {
    field id() -> &str { &self.id }
});

graphql_object!(Droid: Database as "Droid" |&self| {
    field id() -> &str { &self.id }
});

// You can introduce lifetimes or generic parameters by < > before the name.
graphql_interface!(<'a> &'a Character: Database as "Character" |&self| {
    field id() -> &str { self.id() }

    instance_resolvers: |&context| {
        &Human => context.humans.get(self.id()),
        &Droid => context.droids.get(self.id()),
    }
});

# fn main() { }
```

[1]: macro.graphql_object!.html

*/
#[macro_export(local_inner_macros)]
macro_rules! graphql_interface {

    (
        @generate,
        meta = {
            lifetimes = [$($lifetimes:tt,)*],
            name = $name:ty,
            ctx = $ctx:ty,
            main_self = $main_self:ident,
            outname = {$($outname:tt)*},
            scalar = {$($scalar:tt)*},
            $(description = $desciption:tt,)*
            additional = {
                resolver = {
                    $(context = $resolver_ctx: ident,)*
                    items = [
                        $({
                            src = $resolver_src: ty,
                            resolver = $resolver_expr: expr,
                        },)*
                    ],
                 },
            },
        },
        items = [$({
            name = $fn_name: ident,
            body = $body: block,
            return_ty = $return_ty: ty,
            args = [$({
                arg_name = $arg_name : ident,
                arg_ty = $arg_ty: ty,
                $(arg_description = $arg_description: expr,)*
                $(arg_default = $arg_default: expr,)*
            },)*],
            $(decs = $fn_description: expr,)*
            $(docstring = $docstring: expr,)*
            $(deprecated = $deprecated: expr,)*
            $(executor_var = $executor: ident,)*
        },)*],
    ) => {
        __juniper_impl_trait!(
            impl<$($scalar)* $(, $lifetimes)* > GraphQLType for $name {
                type Context = $ctx;
                type TypeInfo = ();

                fn name(_ : &Self::TypeInfo) -> Option<&str> {
                    Some($($outname)*)
                }

                fn meta<'r>(
                    info: &Self::TypeInfo,
                    registry: &mut $crate::Registry<'r, __juniper_insert_generic!($($scalar)+)>
                ) -> $crate::meta::MetaType<'r, __juniper_insert_generic!($($scalar)+)>
                where for<'__b> &'__b __juniper_insert_generic!($($scalar)+): $crate::ScalarRefValue<'__b>,
                    __juniper_insert_generic!($($scalar)+): 'r
                {
                    // Ensure all child types are registered
                    $(
                        let _ = registry.get_type::<$resolver_src>(info);
                    )*
                    let fields = &[$(
                        registry.field_convert::<$return_ty, _, Self::Context>(
                            &$crate::to_camel_case(__graphql__stringify!($fn_name)),
                            info
                        )
                            $(.description($fn_description))*
                            $(.push_docstring($docstring))*
                            $(.deprecated($deprecated))*
                            $(.argument(
                                __juniper_create_arg!(
                                    registry = registry,
                                    info = info,
                                    arg_ty = $arg_ty,
                                    arg_name = $arg_name,
                                    $(description = $arg_description,)*
                                    $(default = $arg_default,)*
                                )
                            ))*,
                    )*];
                    registry.build_interface_type::<$name>(
                        info, fields
                    )
                        $(.description($desciption))*
                        .into_meta()
                }


                #[allow(unused_variables)]
                fn resolve_field(
                    &$main_self,
                    info: &Self::TypeInfo,
                    field: &str,
                    args: &$crate::Arguments<__juniper_insert_generic!($($scalar)+)>,
                    executor: &$crate::Executor<Self::Context, __juniper_insert_generic!($($scalar)+)>
                ) -> $crate::ExecutionResult<__juniper_insert_generic!($($scalar)+)> {
                    $(
                        if field == &$crate::to_camel_case(__graphql__stringify!($fn_name)) {
                            let result: $return_ty = (|| {
                                $(
                                    let $arg_name: $arg_ty = args.get(&$crate::to_camel_case(stringify!($arg_name)))
                                        .expect(__graphql__concat!(
                                            "Argument ",
                                            __graphql__stringify!($arg_name),
                                            " missing - validation must have failed"
                                        ));
                                )*
                                $(
                                    let $executor = &executor;
                                )*
                                $body
                            })();

                            return $crate::IntoResolvable::into(result, executor.context())
                                .and_then(|res| {
                                    match res {
                                        Some((ctx, r)) => {
                                            executor.replaced_context(ctx)
                                                .resolve_with_ctx(&(), &r)
                                        }
                                        None => Ok($crate::Value::null())
                                    }
                                });
                        }
                    )*

                    __graphql__panic!("Field {} not found on type {}", field, $($outname)*)
                }

                #[allow(unused_variables)]
                fn concrete_type_name(&$main_self, context: &Self::Context, _info: &Self::TypeInfo) -> String {
                    $(let $resolver_ctx = &context;)*

                    $(
                        if ($resolver_expr as ::std::option::Option<$resolver_src>).is_some() {
                            return
                                <$resolver_src as $crate::GraphQLType<_>>::name(&()).unwrap().to_owned();
                        }
                    )*

                    __graphql__panic!("Concrete type not handled by instance resolvers on {}", $($outname)*);
                }

                fn resolve_into_type(
                    &$main_self,
                    _info: &Self::TypeInfo,
                    type_name: &str,
                    _: Option<&[$crate::Selection<__juniper_insert_generic!($($scalar)*)>]>,
                    executor: &$crate::Executor<Self::Context, __juniper_insert_generic!($($scalar)*)>,
                ) -> $crate::ExecutionResult<__juniper_insert_generic!($($scalar)*)> {
                    $(let $resolver_ctx = &executor.context();)*

                    $(
                        if type_name == (<$resolver_src as $crate::GraphQLType<_>>::name(&())).unwrap() {
                            return executor.resolve(&(), &$resolver_expr);
                        }
                    )*

                     __graphql__panic!("Concrete type not handled by instance resolvers on {}", $($outname)*);
                }
            }
        );
    };

    (
        @parse,
        meta = {$($meta:tt)*},
        rest = $($rest:tt)*
    ) => {
        __juniper_parse_field_list!(
            success_callback = graphql_interface,
            additional_parser = {
                callback = __juniper_parse_instance_resolver,
                header = {},
            },
            meta = {$($meta)*},
            items = [],
            rest = $($rest)*
        );
    };

    (@$($stuff:tt)*) => {
        __graphql__compile_error!("Invalid syntax for `graphql_interface!`");
    };

    (
        $($rest:tt)*
    ) => {
        __juniper_parse_object_header!(
            callback = graphql_interface,
            rest = $($rest)*
        );
    }


}
