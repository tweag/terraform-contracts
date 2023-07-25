use codespan::Files;
use nickel_lang_core::{
    identifier::Ident,
    parser::utils::{build_record, FieldPathElem},
    position::{RawSpan, TermPos},
    term::{
        record::{self, FieldMetadata, RecordAttrs, RecordData},
        LabeledType, MergePriority, RichTerm, Term,
    },
    typ::{self, EnumRows, RecordRows, TypeF},
};

type StaticPath = Vec<Ident>;

// This is horrible. But Nickel assumes in various places that `TermPos` are
// set, for example when building record fields piece by piece. This will work as
// long as the resulting Nickel term is only pretty printed. Expect evaluation to
// fail horribly on any error.
fn fake_termpos() -> TermPos {
    TermPos::Original(RawSpan {
        src_id: Files::default().add("<fake>", ""),
        start: 0.into(),
        end: 0.into(),
    })
}

pub struct Incomplete();

pub struct Complete(Option<RichTerm>);

#[derive(Debug)]
pub struct Field<RB> {
    record: RB,
    path: StaticPath,
    metadata: FieldMetadata,
}

pub struct Type(pub TypeF<Box<Type>, RecordRows, EnumRows>);

impl From<TypeF<Box<Type>, RecordRows, EnumRows>> for Type {
    fn from(value: TypeF<Box<Type>, RecordRows, EnumRows>) -> Self {
        Type(value)
    }
}

impl From<Type> for typ::Type {
    fn from(t: Type) -> Self {
        Self {
            typ: t
                .0
                .map(|ty| Box::new(Self::from(*ty)), |rrow| rrow, |erow| erow),
            pos: TermPos::None,
        }
    }
}

impl<A> Field<A> {
    pub fn doc(self, doc: impl AsRef<str>) -> Self {
        self.some_doc(Some(doc))
    }

    pub fn some_doc(mut self, some_doc: Option<impl AsRef<str>>) -> Self {
        self.metadata.doc = some_doc.map(|d| d.as_ref().to_owned());
        self
    }

    pub fn optional(self) -> Self {
        self.set_optional(true)
    }

    pub fn set_optional(mut self, opt: bool) -> Self {
        self.metadata.opt = opt;
        self
    }

    pub fn not_exported(self) -> Self {
        self.set_not_exported(true)
    }

    pub fn set_not_exported(mut self, not_exported: bool) -> Self {
        self.metadata.not_exported = not_exported;
        self
    }

    pub fn contract(mut self, contract: impl Into<Type>) -> Self {
        self.metadata.annotation.contracts.push(LabeledType {
            typ: typ::Type::from(contract.into()),
            label: Default::default(),
        });
        self
    }

    pub fn contracts<I>(mut self, contracts: I) -> Self
    where
        I: IntoIterator<Item = Type>,
    {
        self.metadata
            .annotation
            .contracts
            .extend(contracts.into_iter().map(|c| LabeledType {
                typ: c.into(),
                label: Default::default(),
            }));
        self
    }

    pub fn types(mut self, t: impl Into<Type>) -> Self {
        self.metadata.annotation.typ = Some(LabeledType {
            typ: typ::Type::from(t.into()),
            label: Default::default(),
        });
        self
    }

    pub fn priority(mut self, priority: MergePriority) -> Self {
        self.metadata.priority = priority;
        self
    }

    pub fn metadata(mut self, metadata: FieldMetadata) -> Self {
        self.metadata = metadata;
        self
    }
}

impl Field<Incomplete> {
    pub fn path<I, It>(path: It) -> Self
    where
        I: AsRef<str>,
        It: IntoIterator<Item = I>,
    {
        Field {
            record: Incomplete(),
            path: path.into_iter().map(|e| e.as_ref().into()).collect(),
            metadata: Default::default(),
        }
    }

    pub fn name(name: impl AsRef<str>) -> Self {
        Self::path([name])
    }

    pub fn no_value(self) -> Field<Complete> {
        Field {
            record: Complete(None),
            path: self.path,
            metadata: self.metadata,
        }
    }

    pub fn value(self, value: impl Into<RichTerm>) -> Field<Complete> {
        Field {
            record: Complete(Some(value.into())),
            path: self.path,
            metadata: self.metadata,
        }
    }
}

impl Field<Complete> {
    pub fn with_record(self, r: Record) -> Record {
        let v = self.record;
        let f = Field {
            record: r,
            path: self.path,
            metadata: self.metadata,
        };
        match v {
            Complete(Some(v)) => f.value(v),
            Complete(None) => f.no_value(),
        }
    }
}

impl Field<Record> {
    pub fn no_value(mut self) -> Record {
        self.record.fields.push((
            self.path,
            record::Field {
                metadata: self.metadata,
                ..Default::default()
            },
        ));
        self.record
    }

    pub fn value(mut self, value: impl Into<RichTerm>) -> Record {
        self.record.fields.push((
            self.path,
            record::Field {
                value: Some(value.into()),
                metadata: self.metadata,
                ..Default::default()
            },
        ));
        self.record
    }
}

#[derive(Debug)]
pub struct Record {
    fields: Vec<(StaticPath, record::Field)>,
    attrs: RecordAttrs,
}

fn elaborate_field_path(
    path: StaticPath,
    content: record::Field,
) -> (FieldPathElem, record::Field) {
    let mut it = path.into_iter();
    let fst = it.next().unwrap();

    let content = it.rev().fold(content, |acc, id| {
        record::Field::from(RichTerm::from(Term::Record(RecordData {
            fields: [(id, acc)].into(),
            ..Default::default()
        })))
    });

    (FieldPathElem::Ident(fst), content)
}

impl Record {
    pub fn new() -> Self {
        Record {
            fields: vec![],
            attrs: Default::default(),
        }
    }

    pub fn field(self, name: impl AsRef<str>) -> Field<Record> {
        Field {
            record: self,
            path: vec![Ident::new_with_pos(name, fake_termpos())],
            metadata: Default::default(),
        }
    }

    pub fn fields<I, It>(mut self, fields: It) -> Self
    where
        I: Into<Field<Complete>>,
        It: IntoIterator<Item = I>,
    {
        for f in fields {
            self = f.into().with_record(self)
        }
        self
    }

    pub fn path<It, I>(self, path: It) -> Field<Record>
    where
        I: AsRef<str>,
        It: IntoIterator<Item = I>,
    {
        Field {
            record: self,
            path: path
                .into_iter()
                .map(|e| Ident::new_with_pos(e, fake_termpos()))
                .collect(),
            metadata: Default::default(),
        }
    }

    pub fn attrs(mut self, attrs: RecordAttrs) -> Self {
        self.attrs = attrs;
        self
    }

    // Clippy correctly observes that `open` is the only field in `RecordAttrs`.
    // Nevertheless, we want to do the record update. That way we can be
    // somewhat futureproof in case new fields are added to `RecordAttrs` in
    // Nickel.
    #[allow(clippy::needless_update)]
    pub fn open(mut self) -> Self {
        self.attrs = RecordAttrs {
            open: true,
            ..self.attrs
        };
        self
    }

    // See `open` for comments on the clippy directive
    #[allow(clippy::needless_update)]
    pub fn set_open(mut self, open: bool) -> Self {
        self.attrs = RecordAttrs { open, ..self.attrs };
        self
    }

    pub fn build(self) -> RichTerm {
        let elaborated = self
            .fields
            .into_iter()
            .map(|(path, rt)| elaborate_field_path(path, rt))
            .collect::<Vec<_>>();
        build_record(elaborated, self.attrs).into()
    }
}

impl Default for Record {
    fn default() -> Self {
        Self::new()
    }
}

impl<I, It> From<It> for Record
where
    I: Into<Field<Complete>>,
    It: IntoIterator<Item = I>,
{
    fn from(f: It) -> Self {
        Record::new().fields(f)
    }
}

impl From<Record> for RichTerm {
    fn from(val: Record) -> Self {
        val.build()
    }
}

#[cfg(test)]
mod tests {
    use nickel_lang_core::{
        parser::utils::{build_record, FieldPathElem},
        term::{RichTerm, TypeAnnotation},
        typ::{Type, TypeF},
    };

    use pretty_assertions::assert_eq;

    use super::*;

    fn term(t: Term) -> record::Field {
        record::Field::from(RichTerm::from(t))
    }

    #[test]
    fn trivial() {
        let t: RichTerm = Record::new()
            .field("foo")
            .value(Term::Str("bar".into()))
            .into();
        assert_eq!(
            t,
            build_record(
                vec![(
                    FieldPathElem::Ident(Ident::new_with_pos("foo", fake_termpos())),
                    term(Term::Str("bar".to_owned().into()))
                )],
                Default::default()
            )
            .into()
        );
    }

    #[test]
    fn from_iter() {
        let t: RichTerm = Record::from([
            Field::name("foo").value(Term::Null),
            Field::name("bar").value(Term::Null),
        ])
        .into();
        assert_eq!(
            t,
            build_record(
                vec![
                    (
                        FieldPathElem::Ident(Ident::new_with_pos("foo", fake_termpos())),
                        term(Term::Null)
                    ),
                    (
                        FieldPathElem::Ident(Ident::new_with_pos("bar", fake_termpos())),
                        term(Term::Null)
                    ),
                ],
                Default::default()
            )
            .into()
        );
    }

    #[test]
    fn some_doc() {
        let t: RichTerm = Record::from([
            Field::name("foo").some_doc(Some("foo")).no_value(),
            Field::name("bar").some_doc(None as Option<&str>).no_value(),
            Field::name("baz").doc("baz").no_value(),
        ])
        .into();
        assert_eq!(
            t,
            build_record(
                vec![
                    (
                        FieldPathElem::Ident(Ident::new_with_pos("foo", fake_termpos())),
                        record::Field {
                            metadata: FieldMetadata {
                                doc: Some("foo".into()),
                                ..Default::default()
                            },
                            ..Default::default()
                        }
                    ),
                    (
                        FieldPathElem::Ident(Ident::new_with_pos("bar", fake_termpos())),
                        Default::default()
                    ),
                    (
                        FieldPathElem::Ident(Ident::new_with_pos("baz", fake_termpos())),
                        record::Field {
                            metadata: FieldMetadata {
                                doc: Some("baz".into()),
                                ..Default::default()
                            },
                            ..Default::default()
                        }
                    )
                ],
                Default::default()
            )
            .into()
        );
    }

    #[test]
    fn fields() {
        let t: RichTerm = Record::new()
            .fields([
                Field::name("foo").value(Term::Str("foo".into())),
                Field::name("bar").value(Term::Str("bar".into())),
            ])
            .into();
        assert_eq!(
            t,
            build_record(
                vec![
                    (
                        FieldPathElem::Ident(Ident::new_with_pos("foo", fake_termpos())),
                        term(Term::Str("foo".into()))
                    ),
                    (
                        FieldPathElem::Ident(Ident::new_with_pos("bar", fake_termpos())),
                        term(Term::Str("bar".into()))
                    ),
                ],
                Default::default()
            )
            .into()
        );
    }

    #[test]
    fn fields_metadata() {
        let t: RichTerm = Record::new()
            .fields([
                Field::name("foo").optional().no_value(),
                Field::name("bar").optional().no_value(),
            ])
            .into();
        assert_eq!(
            t,
            build_record(
                vec![
                    (
                        FieldPathElem::Ident(Ident::new_with_pos("foo", fake_termpos())),
                        record::Field {
                            metadata: FieldMetadata {
                                opt: true,
                                ..Default::default()
                            },
                            ..Default::default()
                        }
                    ),
                    (
                        FieldPathElem::Ident(Ident::new_with_pos("bar", fake_termpos())),
                        record::Field {
                            metadata: FieldMetadata {
                                opt: true,
                                ..Default::default()
                            },
                            ..Default::default()
                        }
                    ),
                ],
                Default::default()
            )
            .into()
        );
    }

    #[test]
    fn overriding() {
        let t: RichTerm = Record::new()
            .path(vec!["terraform", "required_providers"])
            .value(Record::from([
                Field::name("foo").value(Term::Null),
                Field::name("bar").value(Term::Null),
            ]))
            .path(vec!["terraform", "required_providers", "foo"])
            .value(Term::Str("hello world!".into()))
            .into();
        assert_eq!(
            t,
            build_record(
                vec![
                    elaborate_field_path(
                        vec!["terraform".into(), "required_providers".into()],
                        term(build_record(
                            vec![
                                (
                                    FieldPathElem::Ident(Ident::new_with_pos(
                                        "foo",
                                        fake_termpos()
                                    )),
                                    term(Term::Null)
                                ),
                                (
                                    FieldPathElem::Ident(Ident::new_with_pos(
                                        "bar",
                                        fake_termpos()
                                    )),
                                    term(Term::Null)
                                )
                            ],
                            Default::default()
                        ))
                    ),
                    elaborate_field_path(
                        vec![
                            Ident::new_with_pos("terraform", fake_termpos()),
                            Ident::new_with_pos("required_providers", fake_termpos()),
                            Ident::new_with_pos("foo", fake_termpos())
                        ],
                        term(Term::Str("hello world!".into()))
                    )
                ],
                Default::default()
            )
            .into()
        );
    }

    #[test]
    fn open_record() {
        let t: RichTerm = Record::new().open().into();
        assert_eq!(t, build_record(vec![], RecordAttrs { open: true }).into());
    }

    #[test]
    fn prio_metadata() {
        let t: RichTerm = Record::new()
            .field("foo")
            .priority(MergePriority::Top)
            .no_value()
            .into();
        assert_eq!(
            t,
            build_record(
                vec![(
                    FieldPathElem::Ident("foo".into()),
                    record::Field {
                        metadata: FieldMetadata {
                            priority: MergePriority::Top,
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                )],
                Default::default()
            )
            .into()
        );
    }

    #[test]
    fn contract() {
        let t: RichTerm = Record::new()
            .field("foo")
            .contract(TypeF::String)
            .no_value()
            .into();
        assert_eq!(
            t,
            build_record(
                vec![(
                    FieldPathElem::Ident(Ident::new_with_pos("foo", fake_termpos())),
                    record::Field {
                        metadata: FieldMetadata {
                            annotation: TypeAnnotation {
                                contracts: vec![LabeledType {
                                    typ: Type {
                                        typ: TypeF::String,
                                        pos: TermPos::None
                                    },
                                    label: Default::default()
                                }],
                                ..Default::default()
                            },
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                )],
                Default::default()
            )
            .into()
        );
    }

    #[test]
    fn exercise_metadata() {
        let t: RichTerm = Record::new()
            .field("foo")
            .priority(MergePriority::Bottom)
            .doc("foo?")
            .contract(TypeF::String)
            .types(TypeF::Number)
            .optional()
            .not_exported()
            .no_value()
            .into();
        assert_eq!(
            t,
            build_record(
                vec![(
                    FieldPathElem::Ident(Ident::new_with_pos("foo", fake_termpos())),
                    record::Field {
                        metadata: FieldMetadata {
                            doc: Some("foo?".into()),
                            opt: true,
                            priority: MergePriority::Bottom,
                            not_exported: true,
                            annotation: TypeAnnotation {
                                typ: Some(LabeledType {
                                    typ: Type {
                                        typ: TypeF::Number,
                                        pos: TermPos::None
                                    },
                                    label: Default::default()
                                }),
                                contracts: vec![LabeledType {
                                    typ: Type {
                                        typ: TypeF::String,
                                        pos: TermPos::None
                                    },
                                    label: Default::default()
                                }],
                            },
                        },
                        ..Default::default()
                    }
                )],
                Default::default()
            )
            .into()
        );
    }
}
