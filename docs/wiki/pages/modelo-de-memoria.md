# Modelo de memória

O estudo original recomenda semântica de valor mutável com contagem de referências e elisão em
tempo de compilação, compilada AOT, como o meio-termo entre desempenho e facilidade para LLMs.
A evidência sustenta a escolha, com três correções: a contagem atômica é o custo escondido, a
inferência sozinha não basta e a linguagem precisa de convenções de parâmetro — a lacuna que o
[[piloto-de-vazamento-de-python]] expôs.

## Evidência a favor

- **Perceus** — Reinking, Xie, de Moura e Leijen,
  [PLDI 2021](https://xnning.github.io/papers/perceus.pdf). Contagem de referências com reuso:
  a árvore rubro-negra puramente funcional do Koka (compilado para C) ficou a menos de 10% do
  `std::map` do C++; o Java chegou perto em tempo usando quase 10 vezes a memória (1,7 GiB
  contra 170 MiB). Sem as otimizações de contagem, o Koka fica mais de 2 vezes mais lento, e
  elas rendem menos quando os dados são muito compartilhados.
- **Counting Immutable Beans** — Ullrich e de Moura, [IFL 2019](https://arxiv.org/abs/1908.05647),
  base do Lean 4 (que emite C). Inferência de parâmetros emprestados mais reset/reuso: o Lean
  foi 5 vezes mais rápido que o OCaml em `const_fold`, gastando 17% do tempo em desalocação
  contra 90% do OCaml em coleta.
- **Semântica de valor mutável** — Racordon et al.,
  [JOT 2022](https://www.jot.fm/contents/issue_2022_02/article2.html). Referências são de
  segunda classe: existem só na fronteira das chamadas e nunca são armazenadas; valores de
  tamanho fixo ficam na pilha e contêineres usam copy-on-write. Em 1.344 programas gerados, o
  Swift foi o mais rápido na "overwhelming majority" e só perdeu do C++ com mais de 90% de
  mutações.
- **Quem adotou:** Roc usa contagem de referências com Perceus; Nim usa ORC (contagem com
  coletor de ciclos) por padrão desde a 2.0; Koka e Lean emitem C sem coletor.
- **Ownership no estilo Rust pesa menos do que o estudo original sugere, mas o Rust pesa.**
  Ownership e lifetimes são 16,7% dos erros de compilação de LLMs em Rust
  ([Nogueira et al.](https://arxiv.org/abs/2608.00661)), e o RustAssistant corrige essas
  categorias na mesma taxa que as outras. Mesmo assim, erros de compilação são 94,8% das falhas
  na tradução para Rust ([RustRepoTrans](https://arxiv.org/abs/2411.13990)) e o custo agêntico em
  Rust foi 1,07–1,57 vez o do Python ([Tokenmaxxing](https://arxiv.org/abs/2607.22807)). O que
  torna o Rust difícil para LLMs é o conjunto — traits, resolução de nomes e ownership —, e
  simplificar a resolução de traits importa tanto quanto evitar o borrow checker.

## Os custos escondidos

- **Contagem atômica.** Tornar atômica toda operação de contagem custou de 5% a 59% no Perceus.
  No Swift, a contagem tomou 32% do tempo de execução em média (42% em programas cliente),
  enquanto 87–93% das operações tocavam objetos privados de uma thread
  ([Biased Reference Counting](https://iacoma.cs.uiuc.edu/iacoma-papers/pact18.pdf), PACT 2018).
  O CPython sem GIL usa a mesma técnica de contagem enviesada (PEP 703).
- **A inferência para nas fronteiras.** O Swift precisou acrescentar `borrowing`/`consuming`
  (SE-0377) e tipos `~Copyable` (SE-0390) porque o otimizador não muda convenções fixadas pela
  ABI nem otimiza interfaces polimórficas.
- **Números que não são evidência:** os 95% de operações removidas que o Lobster anuncia são
  autodeclarados, sem metodologia.

## Convenções de parâmetro

Nem o estudo original nem a spec do piloto dizem como uma função altera um valor que pertence a
quem a chamou. No piloto, Sonnet e Opus usaram o único caminho documentado (métodos com
`var self`); o Haiku inventou parâmetros `var` e escreveu testes que esperavam ver o valor do
chamador alterado — semântica de referência do Python. Com semântica de valor, esses testes
falhariam.

A linguagem precisa de convenções explícitas, como o Swift (`inout` com `&` na chamada), o Hylo
e o Mojo (`mut`):

| convenção | significado | na chamada |
| --- | --- | --- |
| padrão | leitura; o compilador empresta sem copiar | `f(x)` |
| `inout` | a função altera o valor do chamador | `f(&x)` — a mutação fica visível |
| `sink` | a função toma posse; o chamador não usa mais | `f(x)`, com o compilador proibindo uso posterior |

Requisitos derivados: o marcador na chamada torna a mutação visível (princípios "sem mágica" e
"performance previsível"); `var self` e `inout` devem ser a mesma ideia com uma só palavra; e o
[[compilador-semantico]] avisa quando um parâmetro copiado é alterado e descartado, sugerindo
`inout` — o erro exato que o piloto mostrou.

## Ciclos

Sem referências armazenadas, ciclos não se formam. Linguagens que permitem referências mutáveis
armazenadas pagam por isso: o Swift exige `weak`/`unowned`, o Nim roda um coletor de ciclos
(ORC), o Lobster relata ciclos ao sair do programa. A proposta original ("proibir ciclos por
design; referências fracas explícitas") está alinhada; grafos ficam com o idioma de arena e
índices, como o grafo por dicionário de adjacência do corpus pareado. `weak` só entra se um caso
real aparecer.

## Meta de desempenho

- **"1–2× o C em benchmarks numéricos" é plausível para código numérico:** Nim fica em 1,0–1,3×
  na maior parte dos benchmarks e em 1,05× no `bench.b` do kostya/benchmarks.
- **Código que aloca muito é onde a contagem perde:** no Benchmarks Game, o Swift 6.0.3 vai de
  1,03× o C (pidigits) a 10,5× (binary-trees) e 21,7× (regex-redux).
- **"10–100× mais rápido que o CPython"** é conservador: o CPython 3.13 fica de 33× a 486× atrás
  do C++ nos benchmarks que não chamam biblioteca C, e o JIT do 3.15 acrescenta 7–8% de média.

O benchmark próprio pedido pelo estudo deve incluir programas que alocam muito, não só
numéricos, e relatá-los separadamente.

## Recomendações

1. Manter semântica de valor com contagem de referências, reuso no estilo Perceus e inferência
   de empréstimo no estilo Lean.
2. Convenções de parâmetro explícitas, com marcador visível na chamada, já na v1 — o destino
   Python também precisa delas ([[estrategia-de-transpilacao]]).
3. Contagem não atômica por padrão; valores que passam entre tarefas são copiados ou movidos,
   como o estudo já prevê — ver [[concorrencia-sem-cor]].
4. Nenhuma referência armazenada; arena e índices na biblioteca padrão.
