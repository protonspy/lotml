# Sintaxe da linguagem X

A sintaxe da linguagem X parte do Python 3.12 e diverge só onde a divergência elimina uma
classe de erro. Esta página registra a regra que decide cada construção, as duas variantes
medidas, o que as medições disseram e as lacunas que a proposta original deixou abertas.

## A regra

O estudo original fixa duas cláusulas, e a evidência acrescenta uma terceira:

1. **Mesma sintaxe ⇒ mesma semântica.** O modelo traz o comportamento do Python junto com a
   palavra.
2. **Semântica diferente ⇒ sintaxe visivelmente diferente.** Por isso `fn` e não `def`: a
   função da linguagem X exige tipos e não lança exceções.
3. **Semântica igual ⇒ a sintaxe do Python.** Copiar o Python não custa tokens — `def` e
   `fn`, `list[int]` e `[int]`, `from … import` e `use` empatam nos oito tokenizadores
   medidos ([[custo-em-tokens]]) — e herda o [[prior-de-treino]].

## Variante A e variante B

A variante A é a proposta original. A variante B troca as construções que violam a regra:

| construção | variante A | variante B | por que a B |
| --- | --- | --- | --- |
| ausência de valor | `none` | `None` | mesma semântica do `None` sob `mypy --strict`; é o que o modelo escreve |
| braço de `match` | `Circle(r):` | `case Circle(r):` | o `match` do Python 3.10 é o mesmo casamento estrutural; `case` custa 1 token |
| padrão de opcional | `x or padrão` | `x ?? padrão` | o `or` do Python cai no padrão com qualquer valor falso (`0`, `""`, `[]`); na A, só com `none`. Mesma sintaxe, semântica diferente. `??` custa o mesmo e tem prior em C#, Swift, Kotlin e JavaScript |
| teste de opcional | `if x:` | `if x is not None:` | mesmo motivo: no Python `if x:` é falso para `0`; custa 3 tokens a mais |
| lambda | `u => u.age` | `lambda u: u.age` | mesma semântica; 1 token a mais |
| import | `use a.b.{c, d}` | `from a.b import c, d` | mesma semântica, mesmo custo; o Python 3 já não tem import relativo implícito (PEP 328), ao contrário do que o estudo original supõe |
| erro esperado em teste | `== fail Minor(15)` | `== Err(Minor(15))` | `fail` como expressão é estranho; `Err` é o construtor que o `match` já expõe |

Ficam iguais nas duas: `fn`, `type` para registros e tipos soma, `T?`, `T ! E`, `?`, `fail`,
`var`, `[T]`, `{K: V}`, `(A, B)`, `impl`, `trait`, blocos `test` e indentação significativa.

## O que as medições disseram

- **Tokens:** a variante B custa 0,8 ponto percentual a mais que a A sobre o corpus pareado
  (0,899–0,915 dos tokens do Python tipado, contra 0,891–0,909 na A) — ver
  [[custo-em-tokens]].
- **Piloto:** as duas empataram — 28 de 30 programas parseiam em cada uma, nenhum vazamento
  sintático de Python em nenhuma — ver [[piloto-de-vazamento-de-python]].
- **O empate desloca a decisão para a semântica.** As armadilhas de `or` e `if x:` na variante
  A não aparecem no parser: um programa que usa `x or 0` com `x = 0` parseia, passa no
  verificador de tipos e se comporta diferente do que o prior do modelo prevê. A variante B
  remove essas duas armadilhas pelo custo de alguns tokens.

**Recomendação:** variante B, confirmada pelo [[harness-de-avaliacao]] com mais modelos e
famílias antes de congelar a v1.

## Lacunas da proposta original

- **Tipo unitário.** Não há forma de declarar uma função que pode falhar e não devolve valor.
  O piloto usou `-> none ! E` (A) e `-> None ! E` (B); a B tem o prior de `-> None`.
- **Campos posicionais em variantes.** O Haiku escreveu `Negate(Expr)` e `Add(Expr, Expr)`,
  como no Rust e no Swift. Aceitar campos posicionais nas variantes remove a classe de erro.
- **`var` em campo de registro.** O Haiku escreveu `type Queue[T](var items: [T])`, como em
  `struct` do Swift. Continua inválido — a mutabilidade é do local, não do campo —, mas pede um
  diagnóstico com a correção pronta.
- **Reatribuição contra sombreamento.** `line = line.strip()` dentro de um laço é hábito de
  Python e apareceu no piloto. Permitir sombreamento o tornaria legal, mas transformaria o
  acumulador `total = total + x` num erro silencioso: criaria um `total` novo a cada volta em vez
  de somar. A regra certa é proibir a reatribuição de imutáveis e sugerir `var`.
- **Como uma função altera um valor do chamador** — ver [[modelo-de-memoria]].
- **`type` e o Python 3.12.** No Python, `type Shape = Circle | Rect` cria um apelido para tipos
  existentes; na linguagem X, `type Shape = Circle(r: f64) | Rect(…)` cria construtores novos.
  É parecido o bastante para ajudar e diferente o bastante para confundir, mas a instrução
  `type` é rara no corpus de treino: risco baixo, a documentar.
- **`int` e `i64`.** O estudo lista `i8`…`i64` e usa `int` nos exemplos. O piloto fixou `int`
  como apelido de `i64`.
- **`if` numa linha só.** O exemplo original escreve `if u.age < 18: fail Minor(u.age)`; uma
  forma canônica única pede bloco sempre ou linha única sempre, e o formatador decide.

## Indentação ou delimitadores

| a favor da indentação | a favor de delimitadores |
| --- | --- |
| prior do Python, e o transpilador para Python fica direto | o SWE-agent precisou de guarda para erros de indentação (flake8 E111–E113) nas edições |
| chaves custam 1 token por bloco — irrelevante ([[custo-em-tokens]]) | o aider criou patch com indentação relativa e patch flexível; sem ele, 9× mais erros de edição |
| nenhum erro de indentação em 60 programas do piloto, escritos inteiros | `INDENT`/`DEDENT` não cabem no subconjunto Lark da OpenAI ([[decodificacao-restrita]]) |

O piloto só mediu programas escritos de uma vez; o risco está em **editar** código existente
com busca-e-substituição ou diffs, que é como agentes trabalham. **Recomendação:** manter a
indentação, com parser tolerante e formatador canônico, e incluir no harness um teste de
edição que compare a variante indentada com uma variante com chaves. Trocar agora é barato;
depois da v1, é migração — por isso a decisão é uma ADR.

## Mantido do estudo original

- **Exigir `return`.** O retorno implícito economiza um token e diverge do Python.
- **Sem pipeline `|>` na v1.** Seria uma segunda forma de chamar função.
