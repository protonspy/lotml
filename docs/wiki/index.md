# Wiki

The entry point. Every page under `wiki/pages/` has to be reachable from here —
directly, or through a page that is — because a page nothing links to is a page
nobody will find again.

Pages link to each other as `[[page-slug]]`, where the slug is the filename without
its extension and without its directory. A link that resolves to no page is reported,
and so is a page this index cannot reach.

This file and `changelog.md` live here rather than in `pages/`: they are the wiki's
fixed documents, not pages, and neither is ever an orphan.

## Pages

### Linguagem X — estudo

- [[linguagem-orientada-a-llms]] — entrada do estudo: conclusões, veredito sobre a proposta original, decisões em aberto

### O que foi medido

- [[custo-em-tokens]] — custo em tokens da sintaxe em oito tokenizadores, corpus pareado e construções isoladas
- [[piloto-de-vazamento-de-python]] — três modelos escrevendo nas variantes A e B a partir da spec

### Contexto

- [[prior-de-treino]] — linguagens sem corpus, confusão com Python e como contornar
- [[linguagens-para-agentes]] — linguagens já desenhadas para LLMs e o que nenhuma mediu

### Design

- [[sintaxe-da-linguagem-x]] — a regra de sintaxe, variantes A e B, lacunas, indentação
- [[sistema-de-tipos]] — tipos verificáveis sobre prefixos, opcionais, overflow, efeitos
- [[modelo-de-memoria]] — semântica de valor, contagem de referências, convenções de parâmetro
- [[concorrencia-sem-cor]] — green threads e onde runtimes sem cor vazam

### Ferramentas

- [[compilador-semantico]] — diagnósticos, loop de correção, digest, LSP e MCP
- [[decodificacao-restrita]] — gramáticas para geração restrita e restrição por tipos

### Plano

- [[estrategia-de-transpilacao]] — destinos Python e C, corpus Python→X
- [[harness-de-avaliacao]] — métricas, tamanho de amostra e portões
- [[riscos-da-linguagem-x]] — tabela de riscos revista
- [[requisitos-e-roadmap]] — requisitos revistos e novos, fases e portões
