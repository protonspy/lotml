---
status: proposed
---

# 0004 · Sintaxe alinhada ao Python onde a semântica coincide

## Context

A proposta original (variante A) usa `none`, braços de `match` sem `case`, `=>`, `use`, e
`x or padrão` e `if x:` para opcionais. Duas dessas formas repetem a sintaxe do Python com
semântica diferente: no Python, `or` e `if` tratam `0`, `""` e `[]` como falsos; na variante A,
só a ausência conta. O parser não pega essa diferença. As outras divergem do Python sem
semântica que justifique. A variante B troca as seis construções pelas do Python ou por formas
sem ambiguidade (`??`, `is not None`). Medido: B custa 0,8 ponto percentual a mais de tokens; no
piloto, A e B empataram em parse (28 de 30) e em vazamento sintático (zero). A sintaxe é o que
todo código, toda doc e todo corpus usarão — mudar depois da v1 é migração. Ver
[[sintaxe-da-linguagem-x]].

## Decision

Adotar a variante B: `None`, `case` nos braços de `match`, `lambda`, `from … import`, `??` como
padrão de opcional, `is None`/`is not None` como teste de opcional e `Err(e)` para comparar com
erro em testes. A regra geral: semântica igual à do Python usa a sintaxe do Python; semântica
diferente usa sintaxe visivelmente diferente. A decisão fica `proposed` até o harness repetir a
comparação com mais modelos e famílias.

## Consequences

- O teste de opcional fica 3 tokens mais caro (`is not None` contra `if x:`).
- `??` é a única forma nova e vem de C#, Swift, Kotlin e JavaScript, não do Python.
- O transpilador para Python fica mais direto: `None`, `lambda`, imports e `match` passam quase
  sem tradução.
- Se o harness mostrar a variante A com menos erros semânticos, esta ADR é rejeitada e a regra
  geral é revista.
