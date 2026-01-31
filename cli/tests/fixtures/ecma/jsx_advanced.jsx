// Advanced JSX patterns

// Simple component
export function SimpleComponent() {
    return <div>Hello</div>;
}

// Component with props
export function WithProps({ name, age }) {
    return <span>{name} is {age} years old</span>;
}

// Component with children
export function Container({ children }) {
    return <div className="container">{children}</div>;
}

// Fragments
export function WithFragment() {
    return (
        <>
            <h1>Title</h1>
            <p>Paragraph</p>
        </>
    );
}

// Conditional rendering
export function Conditional({ show }) {
    return (
        <div>
            {show && <span>Visible</span>}
            {!show && <span>Hidden</span>}
        </div>
    );
}

// List rendering
export function List({ items }) {
    return (
        <ul>
            {items.map((item, index) => (
                <li key={index}>{item}</li>
            ))}
        </ul>
    );
}

// Spread props
export function SpreadProps(props) {
    return <input {...props} />;
}

// Event handlers
export function WithEvents() {
    const handleClick = () => console.log('clicked');
    const handleChange = (e) => console.log(e.target.value);

    return (
        <div>
            <button onClick={handleClick}>Click</button>
            <input onChange={handleChange} />
        </div>
    );
}

// Nested JSX
export function Nested() {
    return (
        <div>
            <header>
                <nav>
                    <ul>
                        <li><a href="/">Home</a></li>
                        <li><a href="/about">About</a></li>
                    </ul>
                </nav>
            </header>
            <main>
                <section>
                    <article>
                        <h2>Title</h2>
                        <p>Content</p>
                    </article>
                </section>
            </main>
            <footer>
                <p>Footer</p>
            </footer>
        </div>
    );
}

// JSX expressions
export function Expressions() {
    const className = 'active';
    const style = { color: 'red' };

    return (
        <div
            className={`container ${className}`}
            style={style}
            data-value={1 + 2}
            aria-label={`Label ${Date.now()}`}
        >
            Content
        </div>
    );
}

// Self-closing tags
export function SelfClosing() {
    return (
        <div>
            <img src="/image.png" alt="Image" />
            <br />
            <hr />
            <input type="text" />
        </div>
    );
}

// Default export component
export default function App() {
    return (
        <div>
            <SimpleComponent />
            <WithProps name="John" age={30} />
            <Container>
                <p>Child content</p>
            </Container>
        </div>
    );
}
