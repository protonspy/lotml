# Prior de treino

Uma linguagem nova começa com corpus zero, e o modelo traz para ela o que aprendeu em outras.
Esta página reúne a evidência sobre esse risco — o maior do estudo original — e as formas
medidas de contorná-lo. A regra de design que sai dela está em [[sintaxe-da-linguagem-x]]:
mesma sintaxe só onde a semântica é a mesma.

## O tamanho do problema

- **Desempenho acompanha a frequência da linguagem no treino.** O
  [MultiPL-E](https://arxiv.org/abs/2208.08227) (Cassano et al.) mediu 18 linguagens além do
  Python e achou correlação significativa com a frequência, com exceções (Lua vai bem). Tipagem
  estática, sozinha, "neither helps nor hinders".
- **Linguagens jovens partem de perto de zero.**
  [Giagnorio et al.](https://arxiv.org/abs/2606.16827) (IEEE TSE, 2026) mediram Gleam e
  MoonBit no McEval-Hard: 0,40–0,97% e 0,88–1,10% zero-shot; 5-shot até 1,32% e 5,46–12,20%;
  RAG pouco melhor; pré-treino continuado 12,47% e 25,86%.
- **Spec no prompt não basta para linguagens muito diferentes.**
  [EsoLang-Bench](https://arxiv.org/abs/2603.09678) (2026) deu a cinco modelos de fronteira a
  spec completa de linguagens esotéricas: o melhor resultado foi 11,2%, contra 100% nos mesmos
  problemas em Python, e três exemplos resolvidos somaram só 0,8 ponto.
- **Agentes fogem para Python.** Modelos de fronteira escrevem programas Python que geram o
  código na linguagem alvo ([arXiv 2606.10933](https://arxiv.org/abs/2606.10933)), e agentes
  prototipam em Python antes de escrever OCaml
  ([Tokenmaxxing](https://arxiv.org/abs/2607.22807)).
- **Sem um "pai" familiar, a saída deriva.** No [SPEAC](https://arxiv.org/abs/2406.03636),
  nenhum modelo produziu UCLID5 que parseasse em 660 tentativas, e as saídas pareciam
  linguagens diferentes a cada vez. Os autores escolheram um subconjunto de Python como
  linguagem-pai e chegaram a 84,8% de parse.
- **Python é o atrator.** Em [Moumoula et al.](https://arxiv.org/abs/2503.13620) (SANER
  2026), a adesão à linguagem pedida variou de 74,33% a 97,60% entre dez modelos, com "a strong
  default to Python"; nomear a linguagem explicitamente levou a adesão a mais de 99%. No
  MojoBench (Raihan, Santos e Zampieri, NAACL Findings 2025) o Claude 3.5 Sonnet respondeu
  em Python quando pedido Mojo.

## O que o piloto mostrou

No [[piloto-de-vazamento-de-python]], três modelos Claude com uma spec de 1.830 tokens no
prompt não escreveram nenhuma construção sintática de Python em 60 programas. O que apareceu,
só no modelo menor, foram hábitos semânticos: reatribuir locais imutáveis, mutar listas sem
`var` e tratar argumentos como referências. Para modelos de fronteira com a spec no contexto,
o risco sai da sintaxe e vai para a semântica.

## Como contornar, em ordem de evidência

1. **Documentação consultável mais compilador no loop.** No Cangjie,
   [Shen et al.](https://arxiv.org/abs/2602.06976) (2026) levaram a geração de 3,23% / 7,74% /
   45,16% zero-shot (DeepSeek-V3.2 / Qwen3-Max / Claude Sonnet 4.5) para 63,23% / 73,55% /
   81,94% dando ao agente documentação e ferramentas do compilador — o maior efeito medido sem
   treinar nada. É o argumento central do [[compilador-semantico]].
2. **Corpus sintético por tradução validada por testes.** O
   [MultiPL-T](https://arxiv.org/abs/2308.09895) (OOPSLA 2024) traduziu funções Python com um
   LLM, manteve só as traduções que passam nos testes e afinou o StarCoderBase-15B: Julia
   21,1→35,2, OCaml 6,9→19,9, Racket 11,8→21,0 de pass@1. O Llama 3 usou a mesma técnica. Uma
   linguagem parecida com Python torna esse tradutor fácil — ver
   [[estrategia-de-transpilacao]].
3. **RL com recompensa verificável.** O [Agnostics](https://arxiv.org/abs/2508.04865) (ICLR
   2026) treinou o Qwen3-4B só com um verificador de entrada e saída e uma configuração curta
   por linguagem: Lua 11→23%, Julia 10→22%, OCaml 1→7%. Compilador e testes bastam.
4. **Pré-treino continuado e fine-tuning** de modelos abertos (números de Gleam e MoonBit
   acima; o Mojo-Coder chegou a 66,4% com 6 milhões de tokens e 3.200 instruções).
5. **Identidade explícita.** Extensão própria, nome da linguagem no prompt e no topo do
   arquivo: barato e eficaz contra confusão.
6. **O compilador reconhece hábitos de Python** e devolve a correção dirigida — `raise` vira
   `fail`, `items = []` seguido de `append` vira `var items = []` — como correção aplicável
   automaticamente.

## Consequências para o projeto

- O tradutor Python→X para gerar corpus deixa de ser opcional: é a principal alavanca para
  modelos abertos. Para modelos fechados, a alavanca é a spec curta mais o compilador semântico.
- A spec curta (meta do estudo original: menos de 10 mil tokens) é viável: a do piloto, que
  cobre o núcleo, tem cerca de 1.830.
- A semântica que diverge do Python — imutável por padrão, semântica de valor — é onde o modelo
  erra sem perceber. Ela precisa de diagnóstico dedicado, não só de documentação.
