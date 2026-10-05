---
status: accepted
---

# 0001 · Transpilar primeiro para Python

## Context

A linguagem lotml ainda não existe, e o que precisa ser validado primeiro é se LLMs a escrevem bem —
não se ela é rápida. O piloto mediu parse e regras estáticas, mas sem executar programas não há
pass@1, que é a métrica primária do projeto. Escrever um backend nativo antes de a sintaxe
estabilizar é trabalho de anos jogado fora a cada mudança. O Python dá, de graça, um runtime, o
ecossistema inteiro e a adoção incremental (chamar lotml a partir de Python), que é como o BAML
contorna o ecossistema vazio. Ver [[estrategia-de-transpilacao]].

## Decision

O primeiro destino da linguagem lotml é Python, por meio de AST do Python emitida com as posições do
fonte original; uma versão mínima entra na fase 0 para o harness executar programas. O C é o
segundo destino, na fase 3, e o backend nativo (Cranelift e LLVM) vem só com a sintaxe estável.
Rejeitados: começar pelo C (atrasa a validação com LLMs e obriga a resolver memória antes da
sintaxe), começar por backend nativo (pior dos dois), e Rust como destino (código gerado cheio de
`Rc`/`clone` e compilação lenta no loop do agente).

## Consequences

- A semântica tem de ser idêntica entre destinos, e o Python não ajuda: inteiros de precisão
  arbitrária exigem emular o trap de `i64` (cerca de 2× numa soma, medição local grosseira) e
  listas por referência exigem cópia ao entrar num `var` ou `inout`.
- Concorrência sem cor não tem equivalente fiel no Python; no destino Python, tarefas viram
  threads.
- O desempenho do destino Python é irrelevante para a decisão e não pode virar argumento para
  pular a fase 3.
- Toda chamada a uma biblioteca Python é falível (`T ! PyError`), porque os stubs não declaram
  exceções.
