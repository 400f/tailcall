# Test query depth with no limit (queryDepth: 0)

```yaml @config
server:
  port: 8001
  queryDepth: 0
upstream:
  httpCache: 42
```

```graphql @schema
schema {
  query: Query
}

type Query {
  user: User @http(url: "http://upstream/user")
}

type User {
  id: Int
  name: String
  posts: [Post]
}

type Post {
  id: Int
  title: String
  comments: [Comment]
}

type Comment {
  id: Int
  text: String
  author: User
}
```

```yml @mock
- request:
    method: GET
    url: http://upstream/user
  response:
    status: 200
    body:
      id: 1
      name: "John"
      posts:
        - id: 1
          title: "Hello"
          comments:
            - id: 1
              text: "Comment"
              author:
                id: 2
                name: "Jane"
                posts: []
```

```yml @test
# Deep query should succeed with no limit
- method: POST
  url: http://localhost:8080/graphql
  body:
    query: |
      {
        user {
          posts {
            comments {
              author {
                name
              }
            }
          }
        }
      }
```
