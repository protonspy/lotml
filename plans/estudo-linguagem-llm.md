---
autonomy: auto
ci: no-wait
---

# Estudo linguagem llm

Aprofundar o estudo da linguagem de programação orientada a LLMs: verificar cada afirmação
contra literatura e medições próprias, testar a sintaxe proposta com um piloto, e destilar
o resultado no wiki do repositório.

## Why

O estudo original é uma proposta sem evidência medida: as metas de tokens, o risco de
"vazamento de Python" e a escolha de sintaxe são opinião até serem medidos. Este trabalho
transforma a proposta em base de decisão — páginas de conceito verificadas, medições
reproduzíveis em `research/` e ADRs propostas para o que é caro de reverter. Termina quando
o wiki cobre o estudo inteiro, a fonte em `docs/raw/` foi processada e o PR está aberto.

## Paths

- `docs/wiki/`
- `docs/adr/`
- `docs/glossary.md`
- `docs/stack.md`
- `research/tokens/`
- `research/pilot/`

## Out of scope

- Implementar o compilador ou o transpilador.
- Fechar as decisões de sintaxe: as ADRs saem como `proposed`, a decisão é do dono do projeto.
- Fine-tuning de modelos ou benchmark com APIs pagas de terceiros.

## Tasks

- [x] 1.1 (Unit) Levantar literatura e estado da arte verificados na web: gramáticas para LLMs, linguagens com pouco corpus, decodificação restrita, compilador no loop, modelos de memória
- [x] 1.2 (TDD) Escrever o contador de tokens multi-tokenizador em `research/tokens/`, com teste que impede tokens especiais de entrarem na contagem
- [x] 1.3 (Unit) Montar o corpus pareado Python tipado × proposta e medir tokens em todos os tokenizadores
  _Depends 1.2_
- [x] 1.4 (Unit) Rascunhar a spec compacta e a gramática Lark das duas variantes de sintaxe em `research/pilot/`
- [x] 1.5 (TDD) Escrever o verificador de parse e vazamento de Python e rodar o piloto de geração com vários modelos
  _Depends 1.4_
- [x] 2.1 (Unit) Registrar os termos canônicos do estudo em `docs/glossary.md`
- [x] 2.2 (Unit) Escrever as páginas de conceito em `docs/wiki/pages/` com o estudo revisado e os resultados
  _Depends 1.1, 1.3, 1.5, 2.1_
- [x] 2.3 (Unit) Registrar ADRs propostas para as decisões difíceis de reverter
  _Depends 2.2_
- [x] 2.4 (Unit) Ligar as páginas no índice, registrar o changelog e remover a fonte de `docs/raw/`
  _Depends 2.2, 2.3_

## Done when

- `scc validate` sai com código 0, sem `wiki.unprocessed-source`.
- Os testes de `research/` passam com `uv run`.
- Toda afirmação numérica no wiki cita uma fonte verificada ou uma medição em `research/`.
- O PR está aberto contra `main`.
