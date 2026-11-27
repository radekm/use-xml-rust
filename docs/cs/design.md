# Návrh

Anglická verze: [../en/design.md](../en/design.md).

`use-xml` extrahuje data z XML dokumentů. Je to rustový protějšek F# knihovny
[UseData.Xml](https://github.com/radekm/UseData.Xml). API nekopíruje originál přesně:

- Žádné closury. Navigační metody vracejí dětské elementy.
- Nepoužitý obsah se nehlásí při zahození elementu. Uživatel explicitně zavolá
  `check_all_used`, která vrátí chybu popisující první nepoužitou položku.
- Atributy a děti jsou uložené v `BTreeMap`, takže pořadí iterace i hlášení chyb je deterministické.
- Namespacy jsou podporované. Elementy a atributy se identifikují namespacem a lokálním jménem.

## Datový model

Celý dokument se naparsuje do `Document`. Ten vlastní arénu uzlů elementů
(v pořadí dokumentu, kořen první) a internovaná jména:

```rust
pub struct Document {
    nodes: Vec<Node>,
    names: NameTable,
}

struct Node {
    name: QualifiedNameHandle,
    raw_name: u32,                 // Jméno, jak je zapsané (s prefixem), používá se v cestách.
    parent: Option<u32>,           // Index rodičovského uzlu.
    index: u32,                    // Index (od 1) mezi sourozenci se stejným kvalifikovaným jménem.
    position: Position,            // Řádek a sloupec počátečního tagu.
    text_significant: bool,
    all_used: Cell<bool>,          // Bylo zavoláno `ignore_all`.
    // `None` po použití textu.
    text: Cell<Option<Box<str>>>,
    // `None` po použití atributu nebo pro vyžádaný chybějící atribut.
    attrs: RefCell<BTreeMap<QualifiedNameHandle, Option<Box<str>>>>,
    // Příznak říká, zda bylo jméno použito. Vyžádaná chybějící jména mají prázdný `Vec`.
    children: RefCell<BTreeMap<QualifiedNameHandle, (bool, Vec<u32>)>>,
}

#[derive(Clone, Copy)]
pub struct Elem<'doc> {
    doc: &'doc Document,
    index: u32,
}

pub struct Position {
    pub line: u32,   // Od 1.
    pub column: u32, // Od 1, počítáno v Unicode scalar values.
}
```

`Elem<'doc>` je malý `Copy` handle. Půjčuje si dokument, takže dokument přežije
všechny své elementy (`Elem`, který by přežil svůj dokument, se nezkompiluje). Rodič
je index do arény, takže dítě se k rodiči dostane bez cyklů referencí.

Hodnoty atributů a texty se při použití přesunou z dokumentu ven, takže jejich použití
nepotřebuje zvláštní příznak: `Some` znamená nepoužitý, `None` použitý. Hodnoty jsou uložené
jako `Box<str>`, který se převede na `String` bez kopírování. Elementy se nepřesouvají;
příznak `bool` v `children` říká, zda bylo jméno dětí použito.

Požadavky na chybějící atribut nebo dítě se zaznamenají do stejných map:
chybějící atribut dostane `None`, chybějící jméno dětí `(true, Vec::with_capacity(0))`.
Jejich kvalifikovaná jména se internují (viz níže), pokud ještě internovaná nejsou.

Všechny metody API berou `self` (`Elem` je `Copy`) nebo `&self` (`Document`). Použití se sleduje
pomocí vnitřní měnitelnosti (`Cell`, `RefCell`), takže uživatel může držet několik elementů
a přitom číst atributy rodiče. `RefCell` není nikdy vypůjčená, zatímco běží uživatelský kód
(např. parser hodnoty). `Document` je proto `Send`, ale ne `Sync`, a `Elem` není ani jedno.

`Debug` pro `Elem` vypíše jen samotný element: cestu, pozici, atributy, počet dětí
pro každé jméno a text (použité hodnoty se zobrazí jako `<used>`). Hodnoty delší než
`MAX_VALUE_CHARS = 100` znaků se zkrátí a končí `…`. `Debug` pro `Document`
vypíše počet elementů a kořen.

Pořadí z dokumentu se zachovává jen mezi dětmi se stejným kvalifikovaným jménem.
Pořadí mezi dětmi s různými jmény se ztrácí (stejně jako v originálu).

### Internování

Jména namespaců (URI) se internují do `NamespaceHandle(u8)` a kvalifikovaná jména
(handle namespacu a lokální jméno) do `QualifiedNameHandle(u32)`.
Handle je index do `Vec`; `BTreeMap` převádí jméno na jeho handle
(řetězce jsou uložené v obou). Kvalifikovaná jména jsou v jedné mapě pro každý namespace,
takže vyhledání podle `(NamespaceHandle, &str)` nealokuje.

Prázdný namespace (`NamespaceHandle::NO_NAMESPACE`, handle 0) a namespace `xml`
(`NamespaceHandle::XML`, handle 1) jsou internované vždy. Každý deklarovaný namespace se internuje
při parsování jeho deklarace.

Po parsování jsou jména z dokumentu neměnná, takže např. `Elem::local_name` vrací `&'doc str`.
Jména internovaná později (`Document::namespace`, kvalifikovaná jména vyžádaných chybějících položek)
jsou ve druhé úrovni v `RefCell`; jejich handly pokračují za handly parseru.

`NamespaceHandle` patří jednomu dokumentu. Použití handlu mimo rozsah dokumentu
je chyba `InvalidNamespaceHandle`; handle jiného dokumentu, který náhodou do rozsahu padne,
odhalit nelze.

Vyžádaná jména se kontrolují stejně jako jména v dokumentu, takže požadavek, který nikdy nemůže
nic najít, je chyba místo tiše chybějící položky:

- `Document::namespace` selže s `LimitExceeded` (`Limit::NamespaceLen`, bez pozice)
  pro jméno namespacu delší než `MAX_NAMESPACE_LEN` a s `InvalidNamespace`
  pro jméno namespacu se znakem, který neodpovídá `Char`,
  nebo pro `http://www.w3.org/2000/xmlns/` (ten se nesmí deklarovat). Prázdný namespace
  a namespace `xml` jsou platné. Nekontroluje se, že jméno namespacu je URI
  (parser to nekontroluje také).
- `attr`, `attr_opt`, `child`, `child_opt` a `children` selžou s `InvalidName`,
  pokud lokální jméno není `NCName` (např. je prázdné nebo obsahuje dvojtečku)
  nebo je delší než `MAX_NAME_LEN`. Neplatná jména se neinternují ani neoznačí jako použitá.

### Cesty

Cesta elementu se formátuje jako XPath ze jmen tak, jak jsou zapsaná v dokumentu
(s prefixy): kořen bez indexu, ostatní elementy s indexem (od 1) mezi sourozenci
se stejným kvalifikovaným jménem, např. `/root/items[1]/item[3]` nebo `/x/p:item[2]`.
Cesty se sestavují jen v případě potřeby (chyby, `Elem::path`) průchodem přes rodiče.

## Pravidla parsování

Parser je vlastní (bez `quick-xml`). Přijímá jen dokumenty, které jsou well-formed
XML 1.0 a namespace-well-formed (Namespaces in XML 1.0) a splňují pravidla níže.
Každé porušení je chyba parsování, ne chyba odhalená později při extrakci.
Parser nepoužívá rekurzi; otevřené elementy jsou na explicitním zásobníku.

### Znaky a jména

- Každý znak musí odpovídat produkci `Char` z XML 1.0 (žádné řídicí znaky kromě
  tabulátoru, line feedu a carriage returnu, žádné `U+FFFE` a `U+FFFF`), a to i když je zapsaný
  jako znaková reference (`&#1;` je chyba).
- Jména elementů, atributů, cíle processing instructions a jména entit musí odpovídat
  produkci `Name`. S namespacy jsou jména elementů a atributů `QName`
  (`prefix:local` nebo `local`, nejvýše jedna dvojtečka, žádná část prázdná);
  cíle processing instructions a jména entit dvojtečku neobsahují.
- Bílé znaky jsou bílé znaky XML: mezera, tabulátor, carriage return a line feed.
- Konce řádků se normalizují: `\r\n` a samostatné `\r` se změní na `\n`.

### Limity

Při překročení limitu parsování skončí chybou `LimitExceeded`:

- `MAX_NAME_LEN = 512` bajtů (ne code pointů) lokálního jména elementu nebo atributu,
  cíle processing instruction a jména entity.
- `MAX_PREFIX_LEN = 64` bajtů prefixu namespacu (i v `xmlns:prefix`).
- `MAX_NAMESPACE_LEN = 4096` bajtů jména namespacu (po dekódování referencí).
- `MAX_NAMESPACES = 256` různých namespaců v dokumentu (včetně prázdného a namespacu `xml`).
  `Document::namespace` selže se stejnou chybou (bez pozice), a také pro jméno namespacu
  delší než `MAX_NAMESPACE_LEN`.
- `u32::MAX` kvalifikovaných jmen.
- `MAX_ATTRS = 1000` atributů jednoho elementu včetně deklarací namespaců.
- `MAX_CHILD_NAMES = 1000` různých kvalifikovaných jmen dětí jednoho elementu.
  Děti se stejným kvalifikovaným jménem se mohou opakovat libovolněkrát.
- `MAX_DEPTH = 256` úrovní zanoření (chyba `MaxDepthExceeded`). Kořenový element má hloubku 1.

### Namespacy

- `xmlns="uri"` deklaruje výchozí namespace, `xmlns=""` ho ruší,
  `xmlns:p="uri"` váže prefix `p`. Deklarace platí ve svém elementu
  (včetně jeho vlastního jména a atributů, i když deklaraci předcházejí)
  a v jeho potomcích.
- Element bez prefixu je ve výchozím namespacu (nebo v žádném).
  Atribut bez prefixu není nikdy v žádném namespacu.
- Prefix `xml` je vázaný na `http://www.w3.org/XML/1998/namespace`. Deklarovat ho lze jen
  s tímto namespacem a tento namespace nesmí být vázaný na jiný prefix ani být výchozím namespacem.
- Prefix `xmlns` se nesmí deklarovat ani použít pro elementy; namespace
  `http://www.w3.org/2000/xmlns/` se nesmí deklarovat.
- Prefix nesmí být vázaný na prázdný namespace (`xmlns:p=""`).
- Nedeklarovaný prefix je chyba.
- Deklarace namespaců nejsou atributy: nelze si je vyžádat a nehlásí se jako nepoužité.
- Jde o chyby `Namespace`, kromě limitů.
- Dva atributy jednoho elementu se stejným zapsaným jménem nebo se stejným namespacem a lokálním jménem
  (např. `a:x` a `b:x`, když jsou `a` i `b` vázané na stejný namespace) jsou chyba `DuplicateAttr`.
  Patří sem i duplicitní deklarace namespaců. `xml:lang` a `lang` jsou různé atributy.

### Obsah

- Smíšený obsah je chyba: element má buď dětské elementy, nebo text, ne obojí.
  Bílé znaky mezi dětskými elementy jsou povolené a ignorují se.
- Pokud má element dětské elementy, jeho text je `""` a není významný.
- Pokud element nemá dětské elementy, jeho text je spojení všech jeho textů a CDATA
  včetně bílých znaků. Text je významný, pokud obsahuje jiný než bílý znak
  nebo alespoň jednu sekci CDATA (i prázdnou). Znaková reference na bílý znak
  je bílý znak.
- Prázdný element (`<a/>` nebo `<a></a>`) má text `""`, který není významný.
- Podporované jsou jen předdefinované entity (`&lt;`, `&gt;`, `&amp;`, `&apos;`, `&quot;`)
  a znakové reference (`&#N;`, `&#xH;`). Ostatní entity jsou chyba `UnknownEntity`.
  Chyby v referencích ukazují na `&`.
- `]]>` není povolené v textu, `<` není povolené v hodnotách atributů.
- Hodnoty atributů se normalizují: doslovný tabulátor, line feed a carriage return
  (`\r\n` jako jeden) se změní na mezery; znaky z referencí zůstanou.
- Mezi atributy je povinný bílý znak. Bílé znaky kolem `=` a před `>` koncového tagu jsou povolené.
- Komentáře nesmějí obsahovat `--`. Komentáře a processing instructions se všude ignorují.
  Cíl processing instruction nesmí být `xml` v libovolné velikosti písmen.

### Dokument

- `parse_str(&str)` parsuje již dekódovaný dokument; kódování v XML deklaraci se kontroluje
  jen syntakticky.
- `parse_bytes(&[u8])` parsuje dokument v kódování UTF-8. Jiná kódování nejsou podporovaná:
  UTF-16 nebo UTF-32 (rozpoznané podle byte order mark nebo podle zakódovaného `<` na začátku),
  jiné kódování než UTF-8 v XML deklaraci (bez ohledu na velikost písmen)
  nebo neplatné UTF-8 vedou k chybě `Encoding`.
- Jeden úvodní byte order mark se přeskočí a nepočítá se do pozic. Druhý je chyba.
- XML deklarace je povolená jen na úplném začátku. Musí obsahovat `version` s hodnotou `1.x`
  a volitelně `encoding` (platné jméno kódování) a `standalone` (`yes` nebo `no`), v tomto pořadí.
  XML deklarace jinde je chyba `UnexpectedNode`.
- Před kořenovým elementem jsou povolené jen XML deklarace, bílé znaky, komentáře a processing instructions.
  Cokoli jiného (`DOCTYPE`, text, reference, CDATA) je chyba `UnexpectedNode`.
- Za kořenovým elementem jsou povolené jen bílé znaky, komentáře a processing instructions.
- Chybné XML (neuzavřené nebo nespárované tagy, neplatná syntaxe) je chyba `Xml`.

## Sledování použití

Za jedno použití se počítá:

- atribut (vyžádaný přes `attr` / `attr_opt`),
- jméno dětí (vyžádané přes `child` / `child_opt` / `children`),
- text (vyžádaný přes `text`).

Pravidla:

- Každou položku lze použít nejvýše jednou. Druhý požadavek selže s chybou „already used“.
- Vyžádání chybějícího atributu nebo dítěte jméno také označí jako použité,
  takže druhý požadavek na něj rovněž selže.
- Vyžádání jména dětí označí jako použité všechny děti s tímto jménem.
  Uživatel vždy dostane všechny (`children`); `child` a `child_opt` navíc kontrolují počet.
- `ignore_all` označí element a všechny jeho potomky jako použité. Každý pozdější požadavek
  na element (nebo jeho potomky) selže s „already used“.

### `check_all_used`

`elem.check_all_used()` (nebo `doc.check_all_used()` pro kořen) projde podstrom do hloubky
a vrátí chybu pro první nepoužitou položku:

1. nepoužitý atribut,
2. nepoužitý významný text (text jen z bílých znaků, který nebyl vyžádán, se nehlásí),
3. nepoužité jméno dětí; chyba má cestu a pozici
   prvního dítěte s tímto jménem, ne rodiče,
4. jinak pokračuje dětmi použitých jmen (v rámci jména v pořadí dokumentu).

Atributy a jména dětí jsou seřazené podle handlů jejich kvalifikovaných jmen, tj. podle prvního výskytu
kvalifikovaného jména v dokumentu. Elementy označené `ignore_all` se přeskočí.
`check_all_used` ani `ignore_all` nepoužívají rekurzi.

## Náčrt API

```rust
let doc = use_xml::parse_str(xml)?;            // Result<Document, Error>
let doc = use_xml::parse_bytes(bytes)?;        // Result<Document, Error>, jen UTF-8

let ns = doc.namespace("urn:x")?;              // NamespaceHandle; internuje se, pokud chybí
let no = doc.namespace("")?;                   // = NamespaceHandle::NO_NAMESPACE
doc.namespace_uri(ns);                         // Option<String>
let root = doc.root();                         // Elem<'_>

root.local_name();                             // &str
root.namespace();                              // NamespaceHandle
root.namespace_uri();                          // &str
root.prefixed_name();                          // &str – jak je zapsané, např. `p:item`
root.parent();                                 // Option<Elem>
root.document();                               // &Document
root.path();                                   // String, např. `/root/items[1]/item[3]`
root.position();                               // Position

root.attr(no, "id", parse::u32)?;              // T, chyba pokud chybí
root.attr_opt(no, "note", parse::string)?;     // Option<T>
root.text(parse::decimal)?;                    // T

root.child(ns, "pig")?;                        // Elem, chyba pokud není právě jedno
root.child_opt(ns, "cat")?;                    // Option<Elem>, chyba pokud je víc než jedno
root.children(ns, "dog")?;                     // Vec<Elem>

root.ignore_all();
root.check_all_used()?;
doc.check_all_used()?;
```

Parser hodnoty je libovolná funkce `FnOnce(String) -> Result<T, E>`, kde `E: Display`.
Hodnota se do parseru přesune, takže `parse::string` ji vrátí bez kopírování;
`str::parse` lze použít jako `|s: String| s.parse::<u8>()`.
Parsery v modulu `parse` vracejí `Result<T, String>`; jejich hlášky obsahují odmítnutou hodnotu
(zkrácenou na `MAX_VALUE_CHARS` znaků), např. `expected u8, got "x"`.
Knihovna chybu obalí kontextem (cesta elementu, pozice, jméno atributu nebo „text“).
Modul `parse` poskytuje hotové parsery převzaté z originálu:
`string`, `string_non_empty`, `string_non_whitespace`, `one_of`, `enumeration`,
celočíselné typy (`i32`, `u32`, `i64`, `u64`, …), `decimal` (`rust_decimal::Decimal`),
`decimal_non_negative`, `bool`, `date_time_utc` a `date_time_offset` (oba `time::OffsetDateTime`).
Pomocné funkce `map` / `validate` parsery kombinují.

Formáty hodnot (všechny striktní, bez okolních bílých znaků):

- Celá čísla: rustové `from_str` – číslice s volitelným `+` nebo `-` na začátku.
- `decimal`: volitelné `+` nebo `-` na začátku, číslice, volitelná `.` následovaná číslicemi.
  Žádný exponent, žádné oddělovače tisíců. Scale je počet číslic za `.`
  (`12.50` zůstane `12.50`). Pokud číslo nelze s tímto scale reprezentovat
  (`Decimal` dovoluje nejvýše 28 číslic za `.`), odstraní se koncové nuly za `.`.
  Čísla, která stále nelze reprezentovat přesně, se odmítnou.
- `bool`: přesně `true` nebo `false`.
- `date_time_utc`: `YYYY-MM-DDTHH:MM:SS[.fffffffff]Z`; místo `T` lze použít mezeru;
  zlomek má 1–9 číslic; jako offset se přijímá jen `Z`.
- `date_time_offset`: totéž, ale offset je `Z` nebo `+HH:MM` / `-HH:MM`.

## Chyby

Knihovna má jeden typ chyby `Error` (vytvořený pomocí `thiserror`). Je to box kolem `ErrorKind`,
takže `Result<T, Error>` zůstává malý; detaily jsou dostupné přes `Error::kind` a `Error::into_kind`
a `Error` lze přímo porovnávat s `ErrorKind`. Varianty `ErrorKind`:

- `Xml` – chybné XML, s pozicí.
- `Namespace` – dokument není namespace-well-formed (nedeklarovaný prefix, rezervovaný prefix nebo namespace,
  neplatné kvalifikované jméno).
- `Encoding` – neplatné UTF-8 nebo nepodporované kódování.
- `UnexpectedNode` – např. `DOCTYPE`, druhý kořenový element, text mimo kořenový element
  nebo XML deklarace, která není na začátku.
- `LimitExceeded` – limit z výše uvedených (`Limit`), s volitelnou pozicí
  (`None` jen z `Document::namespace`).
- `UnknownEntity` – reference na entitu, která není předdefinovaná ani znaková reference;
  pozice ukazuje na `&`.
- `MixedContent`
- `DuplicateAttr` – cesta elementu, pozice a jméno (jak je zapsané) druhého atributu.
- `MaxDepthExceeded` – cesta nejhlubšího povoleného elementu, pozice dítěte, které limit porušilo.
- `InvalidNamespaceHandle` – handle namespacu, který nepatří k dokumentu.
- `InvalidNamespace` – `Document::namespace` dostal jméno namespacu, které se v dokumentu nemůže vyskytnout.
- `InvalidName` – `attr`, `child`, … dostaly lokální jméno, které není `NCName` nebo je příliš dlouhé.
- `Missing` – chybí povinný atribut nebo dítě (`attr`, `child`).
- `WrongCount` – `child` nebo `child_opt` našly víc než jedno dítě.
- `AlreadyUsed`
- `InvalidValue` – parser hodnoty selhal; obsahuje atribut (nebo text) a hlášku.
- `Unused` – hlásí `check_all_used`.

`Error::position()` (a `ErrorKind::position()`) vrací `Option<Position>`; `None` je jen
u `LimitExceeded` z `Document::namespace` a u `InvalidNamespace`.
`Error::path()` (a `ErrorKind::path()`) vrací cestu souvisejícího elementu,
nebo `None` pro `Xml`, `Namespace`, `Encoding`, `UnexpectedNode`, `LimitExceeded`, `UnknownEntity`
a `InvalidNamespace`.

`AlreadyUsed`, `Missing`, `InvalidValue`, `Unused` a `InvalidName` obsahují `Item`:
`Item::Attr(QualifiedName)`, `Item::Child(QualifiedName)` nebo `Item::Text`.
`QualifiedName` obsahuje jméno namespacu a lokální jméno a zobrazuje se jako `local`
pro žádný namespace a jako `{namespace}local` jinak.
