# Linguagem orientada a LLMs

A linguagem X é uma linguagem de programação desenhada para ser escrita por LLMs e revisada por
pessoas: sintaxe próxima do Python, tipos estáticos, erros como valores, semântica de valor e um
compilador pensado como ferramenta de agente. Esta é a página de entrada do estudo — o que foi
verificado, o que mudou em relação à proposta original de 5 de outubro de 2026 e onde está cada
parte.

## Conclusões

1. **A tese se sustenta, mas o gargalo não é a sintaxe.** 94% dos erros de compilação de LLMs
   em código tipado são de tipo; sintaxe é cerca de 6%. O que decide é o
   [[compilador-semantico]] e um [[sistema-de-tipos]] verificável enquanto o código é escrito.
2. **A economia de tokens por sintaxe é de cerca de 10%**, não os 20% da meta original, e em uso
   agêntico pesa menos que uma rodada de correção evitada ([[custo-em-tokens]]).
3. **O risco do corpus mudou de lugar.** Com a spec no prompt, três modelos Claude não escreveram
   nenhuma construção sintática de Python em 60 programas; os erros foram hábitos semânticos —
   mutar imutáveis, tratar argumentos como referência ([[piloto-de-vazamento-de-python]]).
4. **Onde a semântica é a do Python, a sintaxe deve ser a do Python.** A variante B custa 0,8
   ponto percentual a mais de tokens e elimina duas armadilhas que o parser não pega
   ([[sintaxe-da-linguagem-x]]).
5. **A semântica de valor com contagem de referências é a escolha certa**, desde que a
   linguagem tenha convenções de parâmetro explícitas — a lacuna que o piloto expôs — e evite
   contagem atômica ([[modelo-de-memoria]]).

## O estudo original, afirmação por afirmação

| afirmação do estudo original | veredito | onde |
| --- | --- | --- |
| "menos verboso" é menos tokens, não menos caracteres | confirmada | [[custo-em-tokens]] |
| símbolos raros e abreviações custam mais tokens | **corrigida:** glifos APL custam 2–4 tokens; `cnt`, `idx`, `msg` custam 1, como a palavra inteira | [[custo-em-tokens]] |
| o ganho vem de remover boilerplate, não de encurtar nomes | confirmada: um registro economiza 7 tokens, `fn` contra `def` economiza 0 | [[custo-em-tokens]] |
| `str?` custa 1 token em vez de 3–4 | **refutada:** economiza 1 contra `Optional[str]` e contra `str \| None` | [[custo-em-tokens]] |
| a versão proposta tem menos da metade das linhas | verdade em linhas (−62%); em tokens, −12% a −16% | [[custo-em-tokens]] |
| programas ≥ 20% menores em tokens que o Python | **refutada:** 9–11% contra Python tipado em oito tokenizadores | [[custo-em-tokens]] |
| assinaturas obrigatórias e inferência local ajudam mais | confirmada, com ajuste: inferência verificável sobre prefixos | [[sistema-de-tipos]] |
| o compilador é a metade mais importante | confirmada e reforçada: docs mais compilador levaram o Cangjie de 3–45% a 63–82% | [[compilador-semantico]] |
| fixes com confiança numérica | **corrigida:** níveis de aplicabilidade, como no rustc | [[compilador-semantico]] |
| o digest traz mais ganho que enxugar a sintaxe | parcial: melhora localização e custo; a geração ainda precisa dos corpos | [[compilador-semantico]] |
| gramática publicada impede código inválido | **corrigida:** só cobre sintaxe, só OpenAI e modelos abertos aceitam, e gramática incompleta piora a geração | [[decodificacao-restrita]] |
| o risco número um é a falta de corpus | **corrigida:** para modelos de fronteira com a spec, o vazamento é semântico, não sintático | [[prior-de-treino]] |
| nenhuma linguagem foi desenhada para LLMs | **desatualizada:** BAML, MoonBit, Pel, NanoLang, Zero, Quasar — nenhuma com evidência medida | [[linguagens-para-agentes]] |
| `use` evita import relativo implícito | **corrigida:** o Python 3 já não tem import relativo implícito | [[sintaxe-da-linguagem-x]] |
| LLMs erram muito lifetimes no Rust | **corrigida:** ownership é 16,7% dos erros; nomes e traits dominam | [[modelo-de-memoria]] |
| valor mutável com ARC e elisão | confirmada, com contagem não atômica e convenções `inout` | [[modelo-de-memoria]] |
| overflow configurável em release | **corrigida:** uma só semântica em todo build e destino | [[sistema-de-tipos]] |
| concorrência sem cor elimina o `await` esquecido | plausível, não medida | [[concorrencia-sem-cor]] |
| transpilar para Python, depois C, depois nativo | confirmada, com transpilador mínimo já na fase 0 | [[estrategia-de-transpilacao]] |

## Páginas

**O que foi medido**
- [[custo-em-tokens]] — oito tokenizadores, corpus pareado de 12 tarefas, construções isoladas.
- [[piloto-de-vazamento-de-python]] — três modelos, duas variantes, 60 programas.

**O contexto**
- [[prior-de-treino]] — por que linguagens novas partem de perto de zero, e como contornar.
- [[linguagens-para-agentes]] — o que já existe e o que nenhuma delas mediu.

**O design**
- [[sintaxe-da-linguagem-x]] — a regra, as variantes A e B, as lacunas, indentação.
- [[sistema-de-tipos]] — tipos verificáveis sobre prefixos, opcionais, overflow, efeitos.
- [[modelo-de-memoria]] — semântica de valor, contagem de referências, convenções de parâmetro.
- [[concorrencia-sem-cor]] — green threads e onde runtimes sem cor vazam.

**As ferramentas**
- [[compilador-semantico]] — diagnósticos, loop de correção, digest, LSP e MCP.
- [[decodificacao-restrita]] — gramáticas, APIs que as aceitam, restrição por tipos.

**O plano**
- [[estrategia-de-transpilacao]] — destinos Python e C, corpus Python→X.
- [[harness-de-avaliacao]] — métricas, tamanho de amostra, portões.
- [[riscos-da-linguagem-x]] — a tabela de riscos revista.
- [[requisitos-e-roadmap]] — requisitos revistos e novos, fases e portões.

## Decisões em aberto

As decisões caras de reverter estão registradas como ADRs propostas, para o dono do projeto
aceitar ou rejeitar:

- adr:0001-transpilar-primeiro-para-python
- adr:0002-erros-como-valores
- adr:0003-semantica-de-valor-com-contagem-de-referencias
- adr:0004-sintaxe-alinhada-ao-python-onde-a-semantica-coincide
- adr:0005-indentacao-significativa
- adr:0006-compilador-escrito-em-rust

## Como este estudo foi feito

- **Literatura:** três levantamentos independentes (gramáticas e corpus; compilador e
  decodificação; memória e runtime), cada afirmação conferida na fonte primária. Afirmações
  vistas só em resultado de busca ficaram de fora ou estão marcadas.
- **Medições:** `research/tokens/` (contador, corpus pareado, resultados) e `research/pilot/`
  (specs, tarefas, gramática, verificadores, programas gerados, resultados), reproduzíveis com
  `uv run`.
- **Limites:** o tokenizador do Claude não foi medido; o piloto usou só modelos Claude e não
  executou programas; o corpus pareado tem um único autor.
