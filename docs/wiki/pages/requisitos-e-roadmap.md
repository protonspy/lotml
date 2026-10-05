# Requisitos e roadmap

Os requisitos consolidados do estudo original, revistos item a item, mais os que a pesquisa e
as medições acrescentaram, e o roadmap com os portões medidos. Os requisitos marcados v1 formam
o MVP. Esta página é a base para os specs de cada feature; a decisão sobre o que é caro de
reverter está nas ADRs aceitas em `docs/adr/`.

## Requisitos funcionais e de ferramenta

| ID | requisito | fase | situação |
| --- | --- | --- | --- |
| R01 | sintaxe por indentação com o vocabulário do Python | v1 | mantido, condicionado ao teste de edição do [[harness-de-avaliacao]] |
| R02 | tipos obrigatórios em assinaturas, campos e exports; inferência local | v1 | **ajustado:** inferência local e bidirecional, verificável sobre prefixos ([[sistema-de-tipos]]) |
| R03 | registros, tipos soma e `match` exaustivo | v1 | **ajustado:** variantes aceitam campos posicionais |
| R04 | `T?` como única forma de ausência; `T ! E` com `?` e `fail` | v1 | **ajustado:** `??` e `is None`, da variante B (adr:0004-sintaxe-alinhada-ao-python-onde-a-semantica-coincide) |
| R05 | imutável por padrão; `var` para mutável | v1 | mantido |
| R06 | generics monomorfizados e traits | v1 | mantido |
| R07 | blocos `test` junto do código | v1 | mantido; igualdade com erro escrita `== Err(E)` |
| R08 | formatador canônico sem opções | v1 | mantido |
| R09 | diagnósticos em JSON com código estável e correções aplicáveis | v1 | **ajustado:** aplicabilidade em níveis (`MachineApplicable`…) no lugar de confiança numérica; página de explicação por código ([[compilador-semantico]]) |
| R10 | parser tolerante, todos os erros de uma vez | v1 | mantido; causa raiz primeiro, cascatas suprimidas |
| R11 | gramática formal publicada | v1 | **ajustado:** gerada da mesma fonte que o parser, testada contra o corpus, em três dialetos — Lark compatível com llguidance, GBNF e EBNF ([[decodificacao-restrita]]) |
| R12 | especificação completa com menos de 10 mil tokens | v1 | viável: a spec do núcleo no piloto tem cerca de 1.830 |
| R13 | transpilação para Python com erros apontando para o fonte | v1 | **ajustado:** AST do Python com as posições originais; versão mínima já na fase 0 ([[estrategia-de-transpilacao]]) |
| R14 | funções da linguagem chamáveis a partir de Python | v1 | mantido |
| R15 | comando `digest` | v2 | **recomenda-se v1**, com `show <símbolo>` para os corpos: é barato com o verificador de tipos pronto |
| R16 | servidor LSP e MCP | v2 | mantido; refatorações atômicas e consultas textuais além das semânticas |
| R17 | FFI com C | v2 | mantido; chamadas bloqueantes entregues a threads dedicadas |
| R18 | transpilação para C com a mesma semântica do destino Python | v2 | mantido; aritmética verificada no lugar de overflow indefinido, `#line` |
| R19 | semântica de valor com contagem de referências e elisão | v2 | **ajustado:** reuso no estilo Perceus, inferência de empréstimo, contagem não atômica por tarefa ([[modelo-de-memoria]]) |
| R20 | concorrência sem cor | v2 | mantido ([[concorrencia-sem-cor]]) |
| R21 | backend nativo LLVM e Cranelift | v3 | mantido |
| R22 | sistema de efeitos (`io`) | v3 | **ajustado:** começar por capacidade passada como parâmetro; sem evidência de ganho para LLMs |
| R23 | contratos `where` verificados em debug | v3 | mantido |
| R24 | convenções de parâmetro (padrão, `inout`, `sink`) com marcador visível na chamada | v1 | **novo** — o piloto mostrou o modelo inventando semântica de referência |
| R25 | diagnósticos dirigidos a hábitos de Python (mutação de imutável, truthiness, `raise`, argumento por referência) com correção aplicável | v1 | **novo** |
| R26 | uma só semântica de overflow (trap) em todo build e destino, com aritmética modular explícita | v1 | **novo** |
| R27 | bindings tipados gerados dos stubs `.pyi`; toda chamada a Python devolve `T ! PyError` | v2 | **novo** |
| R28 | tipo unitário definido para funções que falham sem devolver valor | v1 | **novo** |

## Requisitos não funcionais

| requisito | situação |
| --- | --- |
| checagem incremental de um arquivo em menos de 100 ms | mantido; é o que permite checar toda edição |
| compilação debug de 10 mil linhas em menos de 2 s | mantido |
| programas ≥ 20% menores em tokens que o Python equivalente, em 3+ tokenizadores | **substituído** por "não maiores que o Python tipado equivalente, em 3+ tokenizadores": a variante A mede 9–11% a menos e o SimPy, 9–14% ([[custo-em-tokens]]) |
| pass@1 igual ou superior ao do Python tipado no mesmo benchmark | mantido; passa a ser a métrica primária |
| desempenho em release dentro de 2× o C em benchmarks numéricos | mantido; benchmarks de alocação relatados à parte |
| parse ≥ 95% e nenhum vazamento sintático para modelos de fronteira com a spec | **novo** (piloto: 93%, 97% com R03 ajustado) |
| mediana de rodadas até passar ≤ 2 com diagnósticos estruturados | **novo** |

## Roadmap

O diagrama de fases do estudo original não veio na exportação para `docs/raw/` (aparece como
"embedded content: roadmap · 5 fases, 4 portões"); as fases abaixo seguem o texto dele, com os
ajustes desta pesquisa. Os critérios de cada portão estão em [[harness-de-avaliacao]].

| fase | entrega | portão para sair |
| --- | --- | --- |
| 0 | harness com tarefas externas e de edição, spec, gramática, verificadores, transpilador mínimo para Python | parse e vazamento; variante e blocos decididos por dado |
| 1 | v1 no destino Python: tipos, `match`, erros, `var` e `inout`, testes, `fmt`, `check --json`/`--fix`, digest | pass@1 ≥ Python tipado; rodadas ≤ 2; tokens ≤ Python tipado |
| 2 | compilador semântico completo (LSP, MCP, refatorações), bindings por stubs, corpus Python→X, concorrência | adoção incremental funcionando; corpus validado |
| 3 | destino C com contagem de referências e reuso, testes de paridade entre destinos | mesma suíte nos dois destinos; ≤ 2× o C |
| 4 | backend nativo (Cranelift em debug, LLVM em release), efeitos, contratos | — |

**Escolhas técnicas do estudo original:** compilador em Rust, parser descendente recursivo
escrito à mão, arquitetura incremental por queries (Salsa), gramática tree-sitter para
editores. Um dado a pesar: o Roc reescreveu o compilador de Rust para Zig para cortar o tempo de
reconstrução incremental do próprio compilador (3,4 s para cerca de 35 ms), ao custo de 487 dias
— a linguagem de implementação é cara de trocar e merece ADR.
