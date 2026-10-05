---
status: accepted
---

# 0002 · Erros como valores

## Context

Exceções deixam o fluxo de erro fora da assinatura: quem chama não sabe, sem ler o corpo, se e
como uma função falha. Para um LLM isso significa adivinhar. Praticantes relatam que agentes
"are afraid of" exceções e preferem resultados tipados, e todas as linguagens recentes feitas
para agentes (BAML, NanoLang, Zero) adotaram erros tipados — sem, porém, publicar evidência
medida ([[linguagens-para-agentes]]). A decisão molda toda API da biblioteca padrão e todo
binding: trocá-la depois é reescrever o ecossistema. Ver [[sistema-de-tipos]].

## Decision

A linguagem X não tem exceções. Uma função que pode falhar declara `T ! E` (açúcar para
`Result[T, E]`), falha com `fail e` e propaga com `expr?`; `x ?? fail e` (ou `x or fail e`,
conforme adr:0004-sintaxe-alinhada-ao-python-onde-a-semantica-coincide) converte ausência em
erro. Pânico existe só para invariantes violados (índice fora do limite, `assert`). Rejeitados:
exceções verificadas como em Java (a mesma informação, com mais cerimônia e sem composição) e
exceções livres como no Python (o que o projeto quer eliminar).

## Consequences

- Assinaturas que falham custam cerca de 3 tokens a mais que a versão Python com exceção oculta,
  e cada propagação custa 1 (`?`) ([[custo-em-tokens]]).
- O modelo tende a escrever `raise`/`try`; o piloto não viu isso com a spec no prompt, mas o
  compilador precisa reconhecer esses hábitos e sugerir a correção.
- Chamadas para Python são todas falíveis (`T ! PyError`).
- É preciso definir o tipo unitário para funções que falham sem devolver valor (R28).
