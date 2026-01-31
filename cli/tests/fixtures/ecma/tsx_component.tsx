// TSX component file
interface ButtonProps {
    label: string;
    onClick: () => void;
}

export function Button({ label, onClick }: ButtonProps) {
    return <button onClick={onClick}>{label}</button>;
}

export default function App() {
    return (
        <div>
            <h1>Hello World</h1>
            <Button label="Click me" onClick={() => {}} />
        </div>
    );
}
