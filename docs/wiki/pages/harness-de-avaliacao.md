# Harness de avaliação

"Sem um benchmark próprio, cada decisão de sintaxe vira opinião", diz o estudo original, que
põe o harness como primeiro entregável. Esta página descreve o harness revisto: o que já existe
em `research/`, o que falta, as métricas que a evidência acrescenta, o tamanho de amostra que
uma decisão exige e os portões do roadmap.

## O que já existe

- **Corpus pareado** (`research/tokens/`): 12 tarefas em Python tipado e nas variantes A e B,
  com contador de tokens em oito tokenizadores — ver [[custo-em-tokens]].
- **Piloto** (`research/pilot/`): duas specs compactas, dez tarefas, gramática Lark das duas
  variantes, verificadores de vazamento sintático e semântico, e a análise que agrega as
  rodadas — ver [[piloto-de-vazamento-de-python]].

## O que falta

1. **Executar.** Sem um transpilador mínimo para Python não há pass@1, só parse e regras
   estáticas ([[estrategia-de-transpilacao]]).
2. **Tarefas de fora.** Traduzir HumanEval e MBPP como o [MultiPL-E](https://arxiv.org/abs/2208.08227)
   faz para 18 linguagens (o trabalho é escrever o tradutor de testes para a linguagem X),
   somar tarefas menos contaminadas como as do LiveCodeBench v6 e tarefas de vários arquivos.
3. **Tarefas de edição.** Aplicar mudanças em código X existente por busca-e-substituição ou
   diff, para decidir indentação contra delimitadores ([[sintaxe-da-linguagem-x]]).
4. **Mais famílias de modelos.** Fechados (Claude, GPT, Gemini) com a spec no prompt; abertos
   (Qwen, Llama, DeepSeek), que devem vazar mais e são os únicos, com a OpenAI, que aceitam
   [[decodificacao-restrita]] por gramática.

## Métricas

As do estudo original — tokens de saída, tokens de contexto, pass@1, rodadas de correção e
vazamento de Python — mais:

- **Laços em código que não compila**, que dominam o custo agêntico
  ([Tokenmaxxing](https://arxiv.org/abs/2607.22807)).
- **Violações semânticas** — mutação de imutável, truthiness, argumento tratado como referência
  —, que o parser não pega e o piloto achou.
- **Rodadas até passar com diagnóstico estruturado contra diagnóstico seco**, a pergunta que a
  literatura não respondeu ([[compilador-semantico]]).
- **Taxa de edição aplicada com sucesso**, por variante de bloco.

## Tamanho de amostra

O piloto teve dez tarefas por célula; uma diferença de uma ou duas falhas ali é ruído. Com
tarefas pareadas (a mesma tarefa nas duas variantes) e o teste de McNemar, a 5% de significância
e 80% de poder:

| diferença a detectar | pares discordantes | tarefas pareadas |
| --- | --- | ---: |
| 10 pontos de pass@1 | 20% | cerca de 155 |
| 5 pontos de pass@1 | 10% | cerca de 310 |

Estimativa pela aproximação normal, n ≈ (1,96·√p + 0,84·√(p − δ²))² / δ², com p a fração de
pares discordantes e δ a diferença. Decidir uma questão de sintaxe pede algumas centenas de
tarefas por comparação e mais de uma amostra por tarefa — não as dezenas que um benchmark do
tipo HumanEval tem sozinho.

## Portões

Cada portão é um critério medido; se não passar, a fase seguinte não começa e a sintaxe é
revista ([[requisitos-e-roadmap]]):

| portão | critério |
| --- | --- |
| 0 → 1 | parse ≥ 95% e nenhum vazamento sintático para modelos de fronteira com a spec; escolha entre variante A e B e entre indentação e chaves feita por dado |
| 1 → 2 | pass@1 na linguagem X ≥ pass@1 em Python tipado nas mesmas tarefas; mediana de rodadas até passar ≤ 2; tokens ≤ Python tipado |
| 2 → 3 | chamar X a partir do Python e Python a partir de X funciona; corpus sintético validado por testes |
| 3 → 4 | suíte inteira passa nos destinos Python e C com o mesmo resultado; ≤ 2× o C nos benchmarks numéricos, com os de alocação relatados à parte |

O piloto atual daria 93% de parse (56 de 60) no primeiro critério; com campos posicionais em
variantes aceitos, daria 97%.
