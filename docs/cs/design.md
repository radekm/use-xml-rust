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
    local_name: String,
    position: Position,                       // Řádek a sloupec počátečního tagu.
    attrs: BTreeMap<String, String>,          // Klíčem je local name.
    attrs_used: RefCell<BTreeSet<String>>,
    children: BTreeMap<String, Vec<Elem>>,    // Klíčem je local name; v rámci jména pořadí z dokumentu.
    children_used: RefCell<BTreeSet<String>>,
    text: String,
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

### Dokument

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

root.name();                                   // &str – local name
root.position();                               // Position

root.attr("id", parse::u32)?;                  // T, chyba pokud chybí
root.attr_opt("note", parse::string)?;         // Option<T>
root.text(parse::decimal)?;                    // T

root.child("pig")?;                            // &Elem, chyba pokud není právě jedno
root.child_opt("cat")?;                        // Option<&Elem>, chyba pokud je jich víc
root.children("dog")?;                         // &[Elem]

root.ignore_all();
root.check_all_used()?;
```

Parser hodnoty je funkce `Fn(&str) -> Result<T, String>`. Knihovna jeho chybu obalí
kontextem (jméno elementu, pozice, jméno atributu nebo „text“, vstupní hodnota).
Modul `parse` nabízí hotové parsery převzaté z originálu:
`string`, `string_non_empty`, `string_non_whitespace`, `one_of`, `enumeration`,
celočíselné typy (`i32`, `u32`, `i64`, `u64`, …), `decimal` (`rust_decimal::Decimal`),
`decimal_non_negative`, `bool` a `date_time_utc` (`time::OffsetDateTime`).
Pomocné funkce `map` / `validate` parsery kombinují.

## Chyby

Knihovna má jeden typ chyby `Error` (vytvořený pomocí `thiserror`). Každá chyba nese
local name a pozici elementu, ke kterému se vztahuje. Plánované varianty:

- `Xml` – chybně utvořené XML (z `quick-xml`), s pozicí.
- `UnexpectedNode` – např. `DOCTYPE` nebo text mimo kořenový element.
- `MixedContent`
- `DuplicateAttr` – atributy se stejným local name.
- `ChildPrefixMismatch` – děti se stejným local name, ale různými prefixy.
- `MaxDepthExceeded`
- `Missing` – chybí povinný atribut nebo dítě.
- `WrongCount` – `child` / `child_opt` našly neočekávaný počet dětí.
- `AlreadyUsed`
- `InvalidValue` – parser hodnoty selhal; obsahuje jméno atributu (nebo text), vstup a zprávu.
- `Unused` – hlásí `check_all_used`.
