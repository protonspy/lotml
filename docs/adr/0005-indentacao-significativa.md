---
status: accepted
---

# 0005 · Indentação significativa

## Context

A linguagem lotml herda a indentação do Python pelo prior de treino, e trocar indentação por chaves
custa só um token por bloco. O piloto não viu nenhum erro de indentação em 60 programas — mas
programas escritos de uma vez. Agentes trabalham editando: o SWE-agent precisou de uma guarda
que desfaz edições com erro de indentação, o aider criou patch com indentação relativa (sem o
patch flexível, 9 vezes mais erros de edição), e o subconjunto Lark que a OpenAI aceita para
geração restrita não consegue expressar `INDENT`/`DEDENT`. Depois da v1, trocar a forma dos
blocos é migrar todo código existente. Ver [[sintaxe-do-lotml]] e
[[decodificacao-restrita]].

## Decision

Manter blocos por indentação, com formatador canônico e parser tolerante que reporta
indentação errada com a correção, **condicionado** ao teste de edição do harness: se a variante
indentada falhar mais em edições aplicadas por agentes que uma variante com chaves, uma nova ADR
substitui esta e a v1 sai com delimitadores explícitos. Rejeitado por ora: decidir sem medir em
qualquer direção.

## Consequences

- O harness precisa de tarefas de edição (busca-e-substituição e diff) antes do portão da fase 0.
- A gramática publicada para geração restrita precisa de dialetos que aceitem indentação
  (GBNF e EBNF com lexer próprio); no subconjunto Lark da OpenAI, a restrição exige uma forma
  alternativa com marcadores de bloco.
- O transpilador para Python fica trivial nesse aspecto.
