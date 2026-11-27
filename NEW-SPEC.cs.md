Protoze aktualni verze vypada naprosto nepouzitelne
a v dusledku kopirovani stringu spotrebovava vic pameti
nez original z F#, je treba vse prepsat.
V nove verzi jsem se navic rozhodl podporovat XML namespacy.

# Vlastni XML parser

Napred bude treba naspat parser XML, protoze quick-xml
naparsuje i neplatne XML (napr. prazdne jmena elementu, hodnoty
atributy obsahujici `<` apod.).
Nove by tedy zadne neplatne XML nemelo byt prijato.

Novy parser bude mit limity.
Nazev elementu a atributu bude omezen konstantou `MAX_NAME_LEN = 512`
bajtu (nikoliv code pointu).
Adresa namespacu bude omezena konstantou `MAX_NAMESPACE_LEN = 4096` bajtu.
A prefix namespacu bude omezen konstantou `MAX_PREFIX_LEN = 64` bajtu.
Pokud nektery z techto limitu bude prekrocen, parsovani konci chybou.

Parser bude namespacy internovat.
Po internovani ziska namespace handle `enum NamespaceHandle(u8)`.
To znamena, ze po precteni a kontrole delky v bajtech se namespace ulozi do
internich struktur `Vec<_>` a `BTreeMap<_, NamespaceHandle>`.
`NamespaceHandle` urcuje index ve vektoru. Mapa prevadi internovany namespace
na jeho handle.
Idealni by bylo, aby se stringy s namespacy neduplikovaly.
Kvuli omezenim Rustu to vsak asi nepujde a stringy budou
jak ve vektoru, tak v mape.

Dale bude parser internovat kvalifikovana jmena elementu a atributu.
Napred se internuje namespace a pak se internuje `(NamespaceHandle, lokalni jmeno)`.
Kvalifikovane jmeno po internovani ziska handle `enum QualifiedNameHandle(u32)`.

Pokud nestaci pocet handlu
(at uz pro namespacy nebo pro jmena elementu a atributu),
tak parser zahlasi chybu a skonci.

Dalsi limit je, ze element nemuze mit vic nez 1000 ruznych kvalifikovanych atributu
a 1000 ruznych kvalifikovanych podelementu. Nicmene stejny kvalifikovany podelement
se muze opakovat, kolikrat chce.

# Nove API s namespacy

Napriklad pro extrakci `taxClass` z dokumentu 

```xml
<x xmlns:edi='http://ecommerce.example.org/schema'>
  <lineItem edi:taxClass="exempt">Baby food</lineItem>
</x>
```

se pouzije nasledujici kod

```rust
// Pokud namespacy neexistuji, tak budou internovany.
// `doc.namespace` skonci chybou jen v pripade, ze internovanych namespacu je prilis.
let default_ns = doc.namespace("")?;
let edi_ns = doc.namespace("http://ecommerce.example.org/schema")?;
let tax_class = root
    .child(default_ns, "lineItem")?
    .attr(edi_ns, "taxClass", use_xml::parse::string)?;
```

# Interni reprezentace

Pokud to pujde: `Elem` obsahuje referenci na dokument a na rodicovsky `Elem`.
Takze by se lifetimy melo osetrit, ze dokument i rodic budou
zit alespon tak dlouho jako dite. Tady se mozna narazi na omezeni Rustu.

U atributu a textu se obsah po pouziti presune pryc, takze nebude treba trackovat pouziti pomoci `Cell`:

```rust
text: RefCell<Option<String>>,  // Nebo `Box<str>` misto `String`?
attrs: RefCell<BTreeMap<QualifiedName, Option<String>>>,
```

Elementy se presouvat nebudou a `bool` priznak u nich zustane:

```rust
children: RefCell<BTreeMap<QualifiedName, (bool, Vec<Elem>)>>,
```

Pouziti chybejicich elementu a atributu se tez zaznamena do map
`attrs` a `children`. V `attrs` se mu priradi hodnota `None`,
v `children` hodnota `(true, Vec::with_capacity(0))`.
