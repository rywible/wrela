# Pinned Unicode 16 identifier data

`DerivedCoreProperties-16.0.0.txt` is the Unicode Consortium's original Unicode
16.0.0 data, retrieved from:
https://www.unicode.org/Public/16.0.0/ucd/DerivedCoreProperties.txt

`python3 frontend/tools/generate_unicode.py` regenerates `frontend/src/unicode16.rs`
without network access or a Unicode library. The generated header includes the
SHA-256 of the original data. The script checks the version header and selects
only `XID_Start`, `XID_Continue`, and `Default_Ignorable_Code_Point` ranges.
The data file carries the Consortium copyright and terms-of-use link.

Identifier starts are XID_Start plus underscore; following scalars are XID_Continue
plus underscore. Both sets exclude every Default_Ignorable_Code_Point, including
join controls U+200C and U+200D. Spelling is retained without normalization.
A forbidden ignorable adjacent to identifier characters makes the entire authored
identifier run one invalid source piece and yields a lexical diagnostic. It is
never stripped or divided into apparently valid names. Strings and comments may
retain these scalars. Unicode 16 additions and non-Unicode-16 characters have
independent lexer tests. Dependencies such as procedural macro tooling may use
newer `unicode-ident` releases; those do not define Wrela identifier recognition.
