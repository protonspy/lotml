# Riscos da linguagem X

A tabela de riscos do estudo original, revista com o que foi medido e pesquisado. Três riscos mudaram de peso e cinco são novos. Cada linha aponta para a página que sustenta a avaliação.

## Riscos do estudo original, revistos

| risco | impacto revisto | o que mudou | mitigação |
| --- | --- | --- | --- |
| corpus de treino inexistente | alto para modelos abertos; médio para modelos de fronteira com a spec | o piloto não achou vazamento sintático em 60 programas; o vazamento migrou para a semântica ([[piloto-de-vazamento-de-python]]) | spec curta mais ferramentas do compilador; corpus por tradução validada; RL com recompensa verificável ([[prior-de-treino]]) |
| ecossistema vazio | alto | nenhuma linguagem gera bindings tipados dos stubs `.pyi`; stubs não declaram exceções | destino Python; bindings gerados do typeshed, com toda chamada a Python devolvendo `T ! PyError` ([[estrategia-de-transpilacao]]) |
| dependência do tokenizador | **baixo** (era médio) | as razões de tokens variaram menos de 2 pontos percentuais entre oito tokenizadores ([[custo-em-tokens]]) | medir em vários tokenizadores continua barato |
| concisão contra legibilidade | médio | sem mudança | princípio 8; formatador canônico |
| modelos evoluem rápido | médio | reforçado: o custo agêntico é dominado por rodadas de correção, não por tokens de superfície | apostar em corretude e no [[compilador-semantico]] |
| tempo de compilação | médio | o frontend incremental pesa mais que o backend (Roc: 35 ms; Cranelift: ~5% do build total no rustc) | arquitetura por queries desde o início |
| escopo do projeto | alto | sem mudança | MVP pequeno; portões medidos ([[harness-de-avaliacao]]) |
| erro reportado em código gerado | alto | precedentes confirmados: AST do Python com posições, `#line` no C | nenhum erro em termos do código gerado |
| concorrência mal gerada | médio, **não medido** | nenhuma fonte mede o "esqueceu o `await`"; runtimes sem cor vazam em FFI bloqueante | concorrência sem cor; entrega de FFI bloqueante a threads dedicadas ([[concorrencia-sem-cor]]) |
| custo da contagem de referências | **médio** (era baixo/médio) | contagem atômica custou 5–59% no Perceus; o Swift gastou 32% do tempo em contagem | contagem não atômica dentro da tarefa; cópia ou move entre tarefas ([[modelo-de-memoria]]) |

## Riscos novos

| risco | impacto | evidência | mitigação |
| --- | --- | --- | --- |
| vazamento semântico de Python | alto | o parser aceita mutação de imutável, truthiness e argumento tratado como referência; o piloto achou os três | diagnósticos dedicados com correção aplicável; variante B; convenção `inout` explícita |
| gramática incompleta | alto, se usada para restringir a geração | restritor incompleto derrubou a corretude em até 97%; a gramática do piloto rejeitou dois programas válidos | gramática gerada da mesma fonte que o parser e testada contra o corpus ([[decodificacao-restrita]]) |
| indentação em edições de agente | médio, não medido | SWE-agent e aider precisaram de proteções para erros de indentação ao editar | teste de edição no harness antes de congelar a v1 ([[sintaxe-da-linguagem-x]]) |
| meta de tokens inalcançável | médio | a variante A dá cerca de 10% a menos que Python tipado; a meta era 20% | trocar a meta por "não pior que Python tipado" ([[requisitos-e-roadmap]]) |
| avaliação enviesada | médio | o corpus pareado tem um único autor; o piloto usou uma família de modelos | tarefas externas, várias famílias, amostras múltiplas |
