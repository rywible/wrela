// One type language, two action domains. Construction retains authored syntax;
// recognition reduces to unit without creating paths, lists, or syntax trees.
pub Type: types::Type = {
    <entry:TypeEntry> <value:TypeForm> => { type_depth.leave(); value }
};
TypeEntry: () = {
    =>? type_depth.enter().map_err(|error| lalrpop_util::ParseError::User { error })
};
TypeForm: types::Type = {
    <l:@L> <path:Path> <arguments:TypeArguments?> <r:@R> => types::named(l, path, arguments, r),
    <l:@L> <open:"("> <contents:TypeCommaList> <close:")"> <r:@R> => types::tuple(l, open, contents, close, r),
    <l:@L> <open:"["> <ty:Type> <close:"]"> <r:@R> => types::array(l, open, ty, close, r)
};
TypeListEntry: () = {
    =>? type_depth.check_next().map_err(|error| lalrpop_util::ParseError::User { error })
};
TypeArguments: types::Arguments = {
    <entry:TypeListEntry> <open:"type<"> <contents:TypeNonemptyCommaList> <close:"type>"> => types::arguments(open, contents, close)
};
CallTypeArguments: types::Arguments = {
    <entry:TypeListEntry> <open:"generic<"> <contents:TypeNonemptyCommaList> <close:"generic>"> => types::arguments(open, contents, close)
};
Path: types::Path = {
    <first:"ident"> <rest:("::" "ident")*> => types::path(first, rest)
};
TypeCommaList: types::List = {
    <ignored:TypeNewlines> <items:TypeMaybeReversedList> => types::restore_order(items)
};
TypeNonemptyCommaList: types::List = {
    <items:TypeReversedNonemptyList> => types::restore_order(items)
};
#[inline]
TypeMaybeReversedList: types::List = {
    => types::empty_list(),
    TypeReversedNonemptyList
};
TypeReversedNonemptyList: types::List = {
    <item:Type> <ignored:TypeNewlines> <rest:TypeReversedRest> => types::push_reversed(item, rest.0, rest.1)
};
TypeReversedRest: (Option<TokenId>, types::List) = {
    => (None, types::empty_list()),
    <comma:","> <ignored:TypeNewlines> <rest:TypeMaybeReversedList> => (Some(comma), rest)
};
#[inline]
TypeNewlines: () = { <ignored:"newline"*> => () };
