# Návrh

Anglická verze: [../en/design.md](../en/design.md).

`use-xml` extrahuje data z XML dokumentů. Je to rustový protějšek F# knihovny
[UseData.Xml](https://github.com/radekm/UseData.Xml). API originál nekopíruje přesně:

- Žádné closures. Navigační metody vracejí reference na podřízené elementy.
- Nepoužitý obsah se nehlásí při zrušení elementu. Uživatel explicitně zavolá
  `check_all_used`, která vrátí chybu popisující první nepoužitou položku.
- Atributy a děti se ukládají do `BTreeMap`, takže pořadí procházení i hlášení chyb je deterministické.

## Datový model

Celý dokument se naparsuje do stromu hodnot `Elem`:

```rust
pub struct Elem {
    path: Arc<ElemPath>,                      // Local name a cesta pro chybové hlášky, např. `/root/items[1]/item[3]`.
    position: Position,                       // Řádek a sloupec počátečního tagu.
    // Klíčem je local name; příznak říká, zda byl atribut použit.
    attrs: BTreeMap<String, (Cell<bool>, String)>,
    // Klíčem je local name; v rámci jména pořadí z dokumentu.
    children: BTreeMap<String, (Cell<bool>, Vec<Elem>)>,
    // Vyžádané atributy a děti, které neexistují.
    // Alokuje se až při prvním takovém požadavku, protože je potřeba jen zřídka.
    missing_used: RefCell<Option<Box<BTreeSet<Item>>>>,
    text: Box<str>,
    // Text obsahuje nebílý znak nebo CDATA sekci.
    text_significant: bool,
    text_used: Cell<bool>,
    // Bylo zavoláno `ignore_all`: vše v elementu včetně potomků je použité
    // a každý další požadavek skončí chybou „already used“.
    all_used: Cell<bool>,
}

pub struct Position {
    pub line: u32,   // Od 1.
    pub column: u32, // Od 1, počítáno v Unicode scalar values.
}
```

`ElemPath` je uzel spojového seznamu `{ parent: Option<Arc<ElemPath>>, name, prefix, index }`
sdílený mezi elementem a jeho potomky, takže každý element stojí jen jednu malou alokaci.
Cesta se formátuje jako XPath: kořen bez indexu, ostatní elementy s indexem od 1
mezi sourozenci se stejným local name, např. `/root/items[1]/item[3]`.

Příznak použití existujícího atributu nebo jména dítěte je uložený vedle hodnoty,
takže jeho použití nealokuje. Do množiny `missing_used` se zapisují jen požadavky
na jména, která neexistují (aby i druhý požadavek skončil chybou).

Všechny metody API berou `&self`. Použití se sleduje vnitřní mutabilitou (`Cell`, `RefCell`),
takže uživatel může držet reference na více dětí a přitom číst atributy rodiče.
`Elem` proto není `Sync`.

Pořadí z dokumentu se zachovává jen mezi dětmi se stejným local name.
Pořadí mezi dětmi s různými jmény se ztrácí (stejně jako v originálu).

## Pravidla parsování

Parsování postaví celý strom a všechna strukturální pravidla zkontroluje hned.
Každé porušení je chyba parsování, ne chyba objevená až později při extrakci.

### Jména a jmenné prostory

- Jmenné prostory se ignorují. Elementy a atributy se identifikují pouze podle local name
  (části za prefixem).
- Atributy `xmlns` a `xmlns:*` se přeskakují.
- Dva atributy jednoho elementu se stejným local name (např. `a:x` a `b:x`) jsou chyba.
- Dvě děti jednoho elementu se stejným local name, ale různými prefixy
  (např. `<a:item/>` a `<b:item/>`) jsou chyba, takže jeden `Vec<Elem>` nikdy nemíchá elementy
  s různými kvalifikovanými jmény. Děti se stejným prefixem (nebo obě bez prefixu) jsou v pořádku.
  Pozn.: výchozí jmenný prostor (`xmlns="..."`) prefix nemění, takže se nedetekuje.

### Obsah

- Smíšený obsah je chyba: element má buď podřízené elementy, nebo text, ne obojí.
  Bílé znaky mezi podřízenými elementy jsou povolené a ignorují se.
- Pokud má element podřízené elementy, jeho text je `""` a není významný.
- Pokud element podřízené elementy nemá, jeho text je spojení všech jeho textů a CDATA
  včetně bílých znaků. Text je významný, pokud obsahuje nebílý znak
  nebo alespoň jednu CDATA sekci (i prázdnou).
- Prázdný element (`<a/>` nebo `<a></a>`) má text `""`, který není významný.
- Entity a znakové reference se dekódují. Podporované jsou pouze předdefinované XML entity
  (`&lt;`, `&gt;`, `&amp;`, `&apos;`, `&quot;`) a znakové reference.
- Komentáře a processing instructions se všude ignorují.
- Bílými znaky se myslí bílé znaky XML: mezera, tabulátor, návrat vozíku a odřádkování.
- Konce řádků v textu a CDATA se normalizují na `\n`.
  Hodnoty atributů se normalizují podle XML (např. konce řádků se změní na mezery).
- Známé omezení: `quick-xml` není plně kontrolující parser. Kromě syntaxe značek
  kontroluje jen komentáře a párování koncových tagů, takže se nedetekuje:
  neplatná jména (např. `<1/>`, `<a 1="x"/>`), jména, která nejsou správně utvořená
  vzhledem ke jmenným prostorům (např. `<a:/>` má prázdné local name, `<a:b:c/>` má local name `b:c`),
  `<` v hodnotách atributů, obsah XML deklarace (např. chybějící `version`),
  zakázané znaky (řídicí znaky, i ze znakových referencí jako `&#1;`)
  a `]]>` v textu. Jde o omezení `quick-xml`, ne o pravidla této knihovny.

### Dokument

- `parse_str(&str)` parsuje již dekódovaný dokument; kódování v XML deklaraci se ignoruje.
- `parse_bytes(&[u8])` parsuje dokument v kódování UTF-8. Jiná kódování podporovaná nejsou:
  UTF-16 nebo UTF-32 (rozpoznané podle byte order mark nebo podle zakódovaného `<` na začátku),
  jiné kódování než UTF-8 v XML deklaraci (bez ohledu na velikost písmen)
  nebo neplatné UTF-8 vedou na chybu `Encoding`.
- Jeden byte order mark na začátku se přeskakuje (dělá to `quick-xml`). Druhý je chyba.

- Před kořenovým elementem je povolena jen XML deklarace, bílé znaky, komentáře a processing instructions.
  Cokoli jiného (včetně `DOCTYPE`) je chyba.
- Za kořenovým elementem jsou povoleny jen bílé znaky, komentáře a processing instructions.
- Chybně utvořené XML (neuzavřené nebo nepárové tagy, neplatná syntaxe) je chyba.
- Hloubka zanoření je omezena `MAX_DEPTH = 256`. Kořenový element má hloubku 1.
  Element v hloubce 257 je chyba. Parsování nepoužívá rekurzi, takže hluboké dokumenty
  nemohou přetéct zásobník.

## Sledování použití

Za jedno použití se počítá každá z těchto položek:

- jméno atributu (vyžádané přes `attr` / `attr_opt`),
- jméno dítěte (vyžádané přes `child` / `child_opt` / `children`),
- text (vyžádaný přes `text`).

Pravidla:

- Každou položku lze použít nejvýše jednou. Druhý požadavek skončí chybou „already used“.
- Vyžádání chybějícího atributu nebo dítěte také označí jméno jako použité,
  takže i druhý požadavek na něj skončí chybou.
- Vyžádání jména dítěte označí jako použité všechny děti s tímto jménem.
  Uživatel vždy dostane všechny (`children`); `child` a `child_opt` navíc kontrolují počet.
- `ignore_all` označí element a všechny jeho potomky jako použité. Každý pozdější požadavek
  na element (nebo jeho potomky) skončí chybou „already used“.

### `check_all_used`

`elem.check_all_used()` projde podstrom do hloubky a vrátí chybu pro první nepoužitou položku:

1. nepoužitý atribut (v pořadí jmen),
2. nepoužitý významný text (text jen z bílých znaků, který nikdo nevyžádal, se nehlásí),
3. nepoužité jméno dítěte (v pořadí jmen),
4. jinak rekurzivně pokračuje do dětí s použitými jmény (v pořadí jmen, pak v pořadí z dokumentu).

Elementy označené `ignore_all` se přeskakují.

## Náčrt API

```rust
let root = use_xml::parse_str(xml)?;           // Result<Elem, Error>
let root = use_xml::parse_bytes(bytes)?;       // Result<Elem, Error>, jen UTF-8

root.name();                                   // &str – local name
root.position();                               // Position

root.attr("id", parse::u32)?;                  // T, chyba pokud chybí
root.attr_opt("note", parse::string)?;         // Option<T>
root.text(parse::decimal)?;                    // T

root.path();                                   // String, např. `/root/items[1]/item[3]`
root.child("pig")?;                            // &Elem, chyba pokud není právě jedno
root.child_opt("cat")?;                        // Option<&Elem>, chyba pokud je jich víc
root.children("dog")?;                         // &[Elem]

root.ignore_all();
root.check_all_used()?;
```

Parser hodnoty je libovolná funkce `FnOnce(&str) -> Result<T, E>`, kde `E: Display`
(funguje tedy např. i `str::parse::<u8>`). Parsery v modulu `parse` vracejí `Result<T, String>`. Knihovna jeho chybu obalí
kontextem (jméno elementu, pozice, jméno atributu nebo „text“, vstupní hodnota).
Modul `parse` nabízí hotové parsery převzaté z originálu:
`string`, `string_non_empty`, `string_non_whitespace`, `one_of`, `enumeration`,
celočíselné typy (`i32`, `u32`, `i64`, `u64`, …), `decimal` (`rust_decimal::Decimal`),
`decimal_non_negative`, `bool`, `date_time_utc` a `date_time_offset` (oba `time::OffsetDateTime`).
Pomocné funkce `map` / `validate` parsery kombinují.

Formáty hodnot (všechny striktní, bez bílých znaků okolo):

- Celá čísla: rustové `from_str` – číslice s volitelným `+` nebo `-` na začátku.
- `decimal`: volitelné `+` nebo `-` na začátku, číslice, volitelně `.` následovaná číslicemi.
  Žádný exponent ani oddělovače tisíců. Scale je počet číslic za `.`
  (`12.50` zůstane `12.50`). Pokud číslo s tímto scale nejde reprezentovat
  (`Decimal` povoluje nejvýše 28 číslic za `.`), odstraní se koncové nuly za `.`.
  Čísla, která ani pak nejde reprezentovat přesně, se odmítnou.
- `bool`: přesně `true` nebo `false`.
- `date_time_utc`: `YYYY-MM-DDTHH:MM:SS[.fffffffff]Z`; místo `T` lze použít mezeru;
  zlomek sekund má 1–9 číslic; jako offset se přijímá pouze `Z`.
- `date_time_offset`: totéž, ale offset je `Z` nebo `+HH:MM` / `-HH:MM`.

## Chyby

Knihovna má jeden typ chyby `Error` (vytvořený pomocí `thiserror`). Je to box kolem `ErrorKind`,
takže `Result<T, Error>` zůstává malý; podrobnosti jsou dostupné přes `Error::kind` a `Error::into_kind`
a `Error` lze přímo porovnávat s `ErrorKind`. Každá chyba
vztahující se k elementu nese cestu a pozici elementu. Varianty `ErrorKind`:

- `Xml` – chybně utvořené XML (z `quick-xml`), s pozicí.
- `Encoding` – neplatné UTF-8 nebo nepodporované kódování (jen z `parse_bytes`).
- `UnexpectedNode` – např. `DOCTYPE`, druhý kořenový element nebo text mimo kořenový element.
- `UnknownEntity` – odkaz na entitu, která není předdefinovaná ani znaková reference.
- `MixedContent`
- `DuplicateAttr` – atributy se stejným local name.
- `ChildPrefixMismatch` – děti se stejným local name, ale různými prefixy
  (cesta rodiče, pozice dítěte, které nesedí).
- `MaxDepthExceeded` – cesta nejhlubšího povoleného elementu, pozice dítěte, které je příliš hluboko.
- `Missing` – chybí povinný atribut nebo dítě (`attr`, `child`).
- `WrongCount` – `child` nebo `child_opt` našly více než jedno dítě.
- `AlreadyUsed`
- `InvalidValue` – parser hodnoty selhal; obsahuje jméno atributu (nebo text), vstup a zprávu.
- `Unused` – hlásí `check_all_used`.

`AlreadyUsed`, `Missing`, `InvalidValue` a `Unused` obsahují `Item`:
`Item::Attr(name)`, `Item::Child(name)` nebo `Item::Text`.
