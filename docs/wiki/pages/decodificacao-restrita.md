# Decodificação restrita

O estudo original pede uma gramática formal publicada (EBNF e GBNF) para que "o modelo nunca
gere código sintaticamente inválido". A evidência confirma o valor da gramática, mas corrige o
alcance: sintaxe é a menor parte dos erros, só uma API hospedada aceita gramática arbitrária, e
uma gramática que rejeita programas válidos é pior que nenhuma.

## O que existe

- **Motores abertos:** GBNF do llama.cpp, [Outlines](https://arxiv.org/abs/2307.09702),
  [XGrammar](https://arxiv.org/abs/2411.15100) (MLSys 2025, "near-zero overhead"),
  [SynCode](https://arxiv.org/abs/2403.01632) (elimina 96,07% dos erros de sintaxe em Python e
  Go), llguidance (cerca de 50 µs por token com vocabulário de 128 mil, usado no Structured
  Outputs da OpenAI, no llama.cpp, no vLLM e no SGLang). O vLLM aceita gramática EBNF com
  vários desses backends.
- **APIs hospedadas, em outubro de 2026:**
  - **OpenAI aceita gramática livre de contexto** em *custom tools*, nas sintaxes `lark` e
    `regex` (regex no dialeto do crate `regex` do Rust), via llguidance, nos modelos GPT-5.
    O subconjunto Lark não aceita lookarounds, quantificadores preguiçosos, prioridade de
    terminais, `%declare` nem imports além de `%import common`, e a API pode recusar uma
    gramática "too complex" sem limite numérico publicado
    ([guia](https://developers.openai.com/api/docs/guides/function-calling)).
  - **Anthropic aceita só JSON Schema**, sem esquemas recursivos
    ([doc](https://platform.claude.com/docs/en/build-with-claude/structured-outputs)).
  - **Google Gemini aceita um subconjunto de JSON Schema**, sem gramática.
  - **Fireworks aceita GBNF** em todos os modelos que serve.

Código-fonte precisa de gramática recursiva: hoje só dá para restringir geração de código da
linguagem lotml em modelos abertos, na OpenAI e na Fireworks — não no Claude nem no Gemini.

## Restringir ajuda ou atrapalha?

- **Distorção:** a restrição por gramática "can distort the LLM's distribution"; o
  [Grammar-Aligned Decoding](https://arxiv.org/abs/2405.21047) (NeurIPS 2024) corrige isso.
- **Formato que atrapalha raciocínio:** no [*Let Me Speak Freely?*](https://arxiv.org/abs/2408.02442)
  (EMNLP 2024 Industry) o GSM8K caiu de 86,51 para 23,44 no Claude 3 Haiku em modo JSON; a
  causa apontada é que as respostas punham a chave da resposta antes da do raciocínio. A
  [réplica da .txt](https://blog.dottxt.ai/say-what-you-mean.html), com prompts equivalentes,
  achou o contrário (0,77 sem restrição, 0,78 com).
- **Restrição bem feita ganha:** no [JSONSchemaBench](https://arxiv.org/abs/2501.10868), todos
  os motores superaram a geração livre (GSM8K com Llama 3.1 8B: 80,1 livre, 83,8 com
  Guidance), e o [CRANE](https://arxiv.org/abs/2502.09061) (ICML 2025) somou até 10 pontos ao
  incluir regras de raciocínio na gramática.
- **Gramática incompleta é desastre:** quando o restritor rejeita programas válidos, a corretude
  funcional cai "by up to 97%" e a geração livre vence
  ([*The Alignment Problem in Constrained Code Generation*](https://arxiv.org/abs/2606.21619),
  2026).

O [[piloto-de-vazamento-de-python]] viu isso em pequena escala: a gramática do piloto rejeitou
dois programas válidos (`**` e literal negativo em padrão) até ser corrigida.

## Tipos valem mais que sintaxe

Restringir por tipos cortou 75,3% (HumanEval) e 52,1% (MBPP) dos erros de compilação em
TypeScript; restringir só a sintaxe, no caso ideal, cortaria 9,0% e 4,8%
([Mündler et al.](https://arxiv.org/abs/2504.09246), PLDI 2025). O reparo melhorou 37%, ao
custo de 39–52% de tempo de decodificação numa implementação Python sem otimização. O
[Monitor-Guided Decoding](https://arxiv.org/abs/2306.10763) (NeurIPS 2023) usou um language
server durante a geração e subiu a taxa de compilação em 19–25%. O MoonBit faz o mesmo no
amostrador. Isso pede um sistema de tipos verificável sobre prefixos — ver [[sistema-de-tipos]].

## Indentação significativa encarece o restritor

O SynCode precisou de maquinário extra no lexer para Python, e as boas práticas da OpenAI pedem
espaço em branco explícito na gramática. Uma linguagem com indentação significativa exige
`INDENT`/`DEDENT` sintetizados, que o subconjunto Lark da OpenAI não oferece (`%declare` não é
aceito). É um argumento concreto a favor de delimitadores explícitos — ver
[[sintaxe-do-lotml]].

## Requisitos para a gramática publicada

1. **A gramática aceita exatamente a linguagem do compilador**, nunca menos: ela é gerada da
   mesma fonte que o parser ou testada contra o mesmo corpus a cada mudança.
2. **Três dialetos:** subconjunto Lark compatível com llguidance (OpenAI), GBNF (llama.cpp,
   Fireworks) e EBNF (vLLM, XGrammar).
3. **Repetição limitada** e sem cadeias de opcionais — o README do GBNF avisa que `x? x? x?`
   pode deixar a amostragem "extremely slow".
4. **Comentários livres** permitidos em qualquer ponto, para o modelo poder raciocinar dentro
   da saída restrita.
5. **Uma API de "continuações válidas e tipo esperado no cursor"** no compilador, para a
   restrição por tipos — o passo que de fato reduz erros.

O protótipo em `research/pilot/check.py` é uma gramática Lark das duas variantes, com
indentação via `Indenter`; ele cumpre o papel de verificador do piloto, não o de gramática
publicada.
